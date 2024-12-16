use crate::app::Command;
use crate::app_model::mailbox::composer::Composer;
use crate::app_model::mailbox::conversations::ConversationsState;
use crate::app_model::mailbox::messages::MessagesState;
use crate::app_model::mailbox::popups::{LabelItemPopup, LabelSelectPopup, MoveItemPopup};
use crate::app_model::mailbox::{Item, Message};
use crate::app_model::watcher::WatchHandle;
use crate::app_model::{AppState, AppStateHandler};
use crate::messages::Messages;
use crate::widgets::CenteredThrobber;
use anyhow::anyhow;
use crossterm::event::{KeyCode, KeyModifiers};
use flume::{Receiver, Sender};
use futures::FutureExt;
use proton_core_common::datatypes::{LabelId, LocalId};
use proton_core_common::models::ModelExtension;
use proton_mail_common::datatypes::{SystemLabelId, ViewMode};
use proton_mail_common::models::{Label, MailSettings};
use proton_mail_common::{
    AppError, MailContext, MailUserContext, Mailbox, MailboxError, MailboxResult,
};
use ratatui::crossterm::event::Event;
use ratatui::layout::{Flex, Rect};
use ratatui::prelude::*;
use std::sync::Arc;
use std::time::Duration;
use throbber_widgets_tui::ThrobberState;
use tracing::error;

enum State {
    Syncing(ThrobberState),
    Conversations(ConversationsState),
    Messages(MessagesState),
}

impl State {
    fn new_syncing() -> Self {
        Self::Syncing(ThrobberState::default())
    }
}

pub(super) trait StateHandler {
    fn handle_event(&mut self, mbox: &Mailbox, event: Event) -> Command<Messages>;

    fn update(
        &mut self,
        ctx: &MailContext,
        message: Message,
        mbox: &Mailbox,
        mail_settings: &Arc<MailSettings>,
    ) -> Command<Messages>;
    fn view(&mut self, frame: &mut Frame, area: Rect);
}
pub struct Model {
    ctx: Arc<MailUserContext>,
    mailbox: Mailbox,
    mail_settings: Arc<MailSettings>,
    label: Label,
    label_watcher: Option<WatchHandle>,
    state: State,
    cancel_token: Option<Sender<()>>,
    composer: Option<Composer>,
}

impl Model {
    pub async fn new(ctx: Arc<MailUserContext>) -> MailboxResult<Self> {
        let mailbox = Mailbox::with_remote_id(ctx.clone(), LabelId::inbox()).await?;
        let tether = ctx.user_stash().connection();
        let label = Label::find_by_id(mailbox.label_id(), &tether)
            .await?
            .ok_or(AppError::LabelNotFound(mailbox.label_id()))?;
        let mail_settings = MailSettings::get(&tether).await?.unwrap_or_default();
        Ok(Self {
            ctx,
            mailbox,
            mail_settings: Arc::new(mail_settings),
            state: State::new_syncing(),
            label,
            cancel_token: None,
            label_watcher: None,
            composer: None,
        })
    }

    fn init_background_worker(&mut self) -> Command<Messages> {
        if self.cancel_token.is_some() {
            return Command::none();
        }

        let ctx = self.ctx.clone();
        let (sender, receiver) = flume::bounded(0);
        self.cancel_token = Some(sender);
        background_worker(ctx, receiver)
    }

    #[must_use]
    fn sync_mailbox(&mut self, mbox: Mailbox) -> Command<Messages> {
        self.state = State::new_syncing();
        // Create the background worker.
        Command::batch([
            self.init_background_worker(),
            Command::task(async {
                let tether = mbox.user_context().user_stash().connection();
                let label = match Label::find_by_id(mbox.label_id(), &tether).await {
                    Ok(l) => {
                        if let Some(l) = l {
                            l
                        } else {
                            let e = anyhow!(
                                "Failed to get label: {}",
                                MailboxError::LabelNotFound(mbox.label_id())
                            );
                            error!("{e}");
                            return Command::message(Messages::DisplayError(None, e));
                        }
                    }
                    Err(e) => {
                        let e = anyhow!("Failed to get label: {e}");
                        error!("{e}");
                        return Command::message(Messages::DisplayError(None, e));
                    }
                };
                if mbox.view_mode() == ViewMode::Conversations {
                    ConversationsState::build(mbox, label)
                } else {
                    MessagesState::build(mbox, label)
                }
            }),
        ])
    }

    fn build_item_count_query(&mut self) -> Command<Messages> {
        let ctx = self.mailbox.user_context();
        let label_id = self.label.local_id.unwrap();
        Command::task(async move {
            let tether = ctx.user_stash().connection();
            let label = Label::find_by_id(label_id, &tether).await;
            let receiver = Label::watch(ctx.user_stash());
            let label_and_recevier = label.and_then(|l| receiver.map(|r| (l, r.receiver)));
            match label_and_recevier {
                Ok((label, receiver)) => {
                    if let Some(label) = label {
                        let (watcher, background_command) = WatchHandle::new(receiver, move |()| {
                            let ctx_clone = ctx.clone();
                            async move {
                                let tether = ctx_clone.user_stash().connection();
                                let label_id = label.local_id.unwrap();

                                Label::find_by_id(label_id, &tether)
                                    .await
                                    .inspect_err(|e| tracing::error!("Failed to get label: `{e}`"))
                                    .ok()
                                    .flatten()
                                    .map(|label| Message::LabelRefreshed(label).into())
                                    .or_else(|| {
                                        tracing::warn!(
                                            "Received change which deleted current label"
                                        );
                                        None
                                    })
                            }
                            .boxed()
                        });
                        Command::batch([
                            Command::message(Message::NewLabelWatcher(watcher).into()),
                            background_command,
                        ])
                    } else {
                        Command::message(
                            MailboxError::from(AppError::LabelNotFound(label_id)).into(),
                        )
                    }
                }
                Err(e) => Command::message(MailboxError::from(e).into()),
            }
        })
    }

    fn open_conversation_view(
        &mut self,
        mbox: Mailbox,
        label: Label,
        state: ConversationsState,
    ) -> Command<Messages> {
        self.mailbox = mbox;
        self.label = label;
        self.state = State::Conversations(state);
        self.build_item_count_query()
    }

    fn open_message_view(
        &mut self,
        mbox: Mailbox,
        label: Label,
        state: MessagesState,
    ) -> Command<Messages> {
        self.mailbox = mbox;
        self.label = label;
        self.state = State::Messages(state);
        self.build_item_count_query()
    }

    fn open_label_select_popup(&mut self) -> Command<Messages> {
        let ctx = self.mailbox.user_context();
        let label = self.label.clone();
        let view_mode = self.mailbox.view_mode();
        Command::task(async move {
            match LabelSelectPopup::new(ctx, &label, view_mode).await {
                Ok(state) => Command::message(Messages::RaisePopup(Box::new(state))),
                Err(e) => {
                    let e = anyhow!("Failed to load labels: {e}");
                    tracing::error!("{e}");
                    Command::message(Messages::DisplayError(None, e))
                }
            }
        })
    }

    fn open_move_item_popup(&mut self, item: Item) -> Command<Messages> {
        if matches!(&self.state, State::Syncing(_)) {
            return Command::None;
        };

        let ctx = self.mailbox.user_context();
        Command::task(async move {
            match MoveItemPopup::new(&ctx, item).await {
                Ok(state) => Command::message(Messages::RaisePopup(Box::new(state))),
                Err(e) => {
                    let e = anyhow!("Failed to load folders: {e}");
                    tracing::error!("{e}");
                    Command::message(e.into())
                }
            }
        })
    }

    fn open_label_popup(&mut self, item: Item) -> Command<Messages> {
        if matches!(&self.state, State::Syncing(_)) {
            return Command::None;
        };

        let ctx = self.mailbox.user_context();
        Command::task(async move {
            match LabelItemPopup::new(&ctx, item).await {
                Ok(state) => Command::message(Messages::RaisePopup(Box::new(state))),
                Err(e) => {
                    let e = anyhow!("Failed to load labels: {e}");
                    tracing::error!("{e}");
                    Command::message(e.into())
                }
            }
        })
    }

    fn select_label(&mut self, label_id: LocalId) -> Command<Messages> {
        let ctx = Arc::clone(&self.ctx);
        Command::task(async move {
            Command::message(match Mailbox::new(ctx, label_id).await {
                Ok(mbox) => Message::Sync(mbox).into(),
                Err(e) => {
                    let e = anyhow!("Failed to open label: {e}");
                    tracing::error!("{e}");
                    Messages::DisplayError(None, e)
                }
            })
        })
    }
}

impl AppStateHandler for Model {
    fn on_state_enter(&mut self) -> Command<Messages> {
        Command::message(Message::Sync(self.mailbox.clone()).into())
    }
    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        if let Some(composer) = &mut self.composer {
            return composer.handle_event(&self.mailbox, event);
        } else if let Event::Key(key) = &event {
            if key.code == KeyCode::Char('n') && key.modifiers.contains(KeyModifiers::CONTROL) {
                let ctx = self.mailbox.user_context();
                return Composer::empty(ctx);
            }
        }

        match &mut self.state {
            State::Syncing(_) => {
                // Do nothing
                Command::None
            }
            State::Conversations(state) => state.handle_event(&self.mailbox, event),
            State::Messages(state) => state.handle_event(&self.mailbox, event),
        }
    }

    fn update(&mut self, ctx: &Arc<MailContext>, message: Messages) -> Command<Messages> {
        let Messages::Mailbox(message) = message else {
            return Command::None;
        };

        if matches!(&message, Message::CloseComposer) {
            self.composer = None;
            return Command::None;
        }

        if let Some(composer) = &mut self.composer {
            return composer.update(ctx, message, &self.mailbox, &self.mail_settings);
        }

        match message {
            Message::Sync(mbox) => self.sync_mailbox(mbox),
            Message::OpenConversationView(mbox, label, state) => {
                self.open_conversation_view(mbox, label, state)
            }
            Message::OpenMessageView(mbox, label, state) => {
                self.open_message_view(mbox, label, state)
            }
            Message::OpenLabelSelectPopup => self.open_label_select_popup(),
            Message::SelectLabel(label_id) => self.select_label(label_id),
            Message::OpenMoveItemPopup(item) => self.open_move_item_popup(item),
            Message::OpenLabelItemPopup(item) => self.open_label_popup(item),
            Message::ConversationState(_) | Message::MessageState(_) => {
                self.state
                    .update(ctx, message, &self.mailbox, &self.mail_settings)
            }
            Message::LabelRefreshed(label) => {
                self.label = label;
                Command::None
            }
            Message::OpenComposer(composer) => {
                self.composer = Some(composer);
                Command::None
            }
            Message::CloseComposer => {
                self.composer = None;
                Command::None
            }
            Message::NewLabelWatcher(handle) => {
                self.label_watcher = Some(handle);
                Command::None
            }
            Message::Composer(_) => Command::None,
        }
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        self.state.view(frame, area);
        if let Some(composer) = &mut self.composer {
            composer.view(frame, area);
        }
    }

    fn view_help_bar(&mut self, frame: &mut Frame, area: Rect) {
        let spans = vec![
            Span::from(" ▲: ").bold(),
            Span::from("Up"),
            Span::from(" ▼: ").bold(),
            Span::from("Down"),
            Span::from(" Enter: ").bold(),
            Span::from("Open"),
            Span::from(" Esc: ").bold(),
            Span::from("Close"),
            Span::from(" Tab: ").bold(),
            Span::from("Toggle"),
            Span::from(" S: ").bold(),
            Span::from("Switch"),
            Span::from(" M: ").bold(),
            Span::from("Move"),
            Span::from(" R: ").bold(),
            Span::from("Read"),
            Span::from(" U: ").bold(),
            Span::from("Unread"),
            Span::from(" l: ").bold(),
            Span::from("Label"),
            Span::from(" D: ").bold(),
            Span::from("Delete"),
            Span::from(" Shift+▲: ").bold(),
            Span::from("Msg. Up"),
            Span::from(" Shift+▼: ").bold(),
            Span::from("Msg. Down"),
            Span::from(" Crtl+N: ").bold(),
            Span::from("New Msg."),
            Span::from(" Crtl+R: ").bold(),
            Span::from("Reply Msg."),
            Span::from(" Crtl+T: ").bold(),
            Span::from("Reply All Msg."),
            Span::from(" Crtl+F: ").bold(),
            Span::from("Forward Msg."),
        ];
        frame.render_widget(Line::from(spans), area);
    }

    fn view_status_bar(&mut self, frame: &mut Frame, area: Rect) {
        let label_name = self
            .label
            .path
            .as_deref()
            .unwrap_or(self.label.name.as_str());

        let (total, unread) = if self.mailbox.view_mode() == ViewMode::Conversations {
            (self.label.total_conv, self.label.unread_conv)
        } else {
            (self.label.total_msg, self.label.unread_msg)
        };
        let counters = format!("T:{total:4} U:{unread:4}");
        let [label_area, _, count_area, other_area] = Layout::horizontal([
            Constraint::Length(u16::try_from(label_name.chars().count()).unwrap_or(10)),
            Constraint::Length(1),
            Constraint::Length(13),
            Constraint::Percentage(100),
        ])
        .flex(Flex::Start)
        .areas(area);
        let text = Text::from(label_name);
        frame.render_widget(text, label_area);
        frame.render_widget(Text::from(counters), count_area);
        if let State::Conversations(state) = &mut self.state {
            state.draw_status_bar(frame, other_area);
        }
    }
}

impl StateHandler for State {
    fn handle_event(&mut self, mbox: &Mailbox, event: Event) -> Command<Messages> {
        match self {
            State::Syncing(_) => Command::None,
            State::Conversations(state) => state.handle_event(mbox, event),
            State::Messages(state) => state.handle_event(mbox, event),
        }
    }

    fn update(
        &mut self,
        ctx: &MailContext,
        message: Message,
        mbox: &Mailbox,
        mail_settings: &Arc<MailSettings>,
    ) -> Command<Messages> {
        match self {
            State::Syncing(_) => Command::None,
            State::Conversations(state) => state.update(ctx, message, mbox, mail_settings),
            State::Messages(state) => state.update(ctx, message, mbox, mail_settings),
        }
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        match self {
            State::Syncing(state) => {
                let throbber = throbber_widgets_tui::Throbber::default()
                    .label("Loading...")
                    .throbber_set(throbber_widgets_tui::BRAILLE_SIX)
                    .use_type(throbber_widgets_tui::WhichUse::Spin);
                frame.render_stateful_widget(CenteredThrobber::new(throbber), area, state);
            }
            State::Conversations(state) => {
                state.view(frame, area);
            }
            State::Messages(state) => {
                state.view(frame, area);
            }
        }
    }
}

impl From<Model> for AppState {
    fn from(value: Model) -> Self {
        Self::Mailbox(value)
    }
}

fn background_worker(context: Arc<MailUserContext>, reader: Receiver<()>) -> Command<Messages> {
    Command::background_task(|sender| {
        async move {
            let mut interval = tokio::time::interval(Duration::from_secs(15));
            loop {
                tokio::select! {
                _ = reader.recv_async() => {
                    return;
                }
                _ = interval.tick() => {
                    if let Err(e) = context.execute_pending_actions().await {
                        let e = anyhow!("Failed to flush actions: {e}");
                        error!("{e}");
                        if sender
                            .send(Command::message(Messages::DisplayError(
                                Some("Action Queue".to_owned()),
                                e,
                            )))
                            .is_err()
                        {
                            error!("Failed to send message from worker");
                        }
                    }

                        if let Err(e) = context.poll_event_loop().await {
                            let e = anyhow!("Failed to poll events: {e}");
                            error!("{e}");
                            if sender
                                .send(Command::message(Messages::DisplayError(
                                    Some("Event Loop".to_owned()),
                                    e,
                                )))
                                .is_err()
                            {
                                error!("Failed to send message from worker");
                            }
                        }
                    }
                }
            }
        }
        .boxed()
    })
}
