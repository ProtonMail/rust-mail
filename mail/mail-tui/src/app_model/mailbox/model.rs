use super::search::{Search, SearchStatusBar};
use crate::CLI_ARGS;
use crate::app::Command;
use crate::app_model::mailbox::composer::Composer;
use crate::app_model::mailbox::conversations::ConversationsState;
use crate::app_model::mailbox::messages::MessagesState;
use crate::app_model::mailbox::popups::{LabelItemPopup, LabelSelectPopup, MoveItemPopup};
use crate::app_model::mailbox::{Item, Message};
use crate::app_model::watcher::WatchHandle;
use crate::app_model::{AppState, AppStateHandler, YesNoPopup};
use crate::messages::Messages;
use crate::widgets::CenteredThrobber;
use anyhow::anyhow;
use crossterm::event::{KeyCode, KeyModifiers};
use flume::Sender;
use futures::FutureExt;

use proton_action_queue::observers::{ActionFailureObserver, ActionFailureReason};
use proton_action_queue::queue::{ActionError, AsActionError};
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::{Label, ModelExtension};
use proton_mail_common::actions::event_poll::EventPoll;
use proton_mail_common::datatypes::{ReadFilter, SystemLabelId, ViewMode};
use proton_mail_common::draft::Draft;
use proton_mail_common::draft::observers::DraftSendResultWatcher;
use proton_mail_common::models::{
    ConversationCounters, DraftSendFailure, DraftSendResult, DraftSendResultOrigin, MailSettings,
    MessageCounters,
};
use proton_mail_common::proton_api_mail::proton_api_core::services::proton::LabelId;
use proton_mail_common::{
    AppError, MailContext, MailUserContext, Mailbox, MailboxError, MailboxResult,
};
use ratatui::crossterm::event::Event;
use ratatui::layout::{Flex, Rect};
use ratatui::prelude::*;
use stash::stash::WatcherHandle;
use std::sync::Arc;
use std::time::Duration;
use throbber_widgets_tui::ThrobberState;
use tokio::select;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};

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
    fn handle_event(
        &mut self,
        user_ctx: &Arc<MailUserContext>,
        mbox: &Mailbox,
        event: Event,
    ) -> Command<Messages>;

    fn update(
        &mut self,
        ctx: &MailContext,
        user_ctx: &Arc<MailUserContext>,
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
    conv_counters: ConversationCounters,
    msg_counters: MessageCounters,
    label_watcher: Option<WatchHandle>,
    state: State,
    cancel_token: CancellationToken,
    composer: Option<Composer>,
    search: Option<Search>,
    search_status: Option<SearchStatusBar>,
    filter: ReadFilter,
    background_worker_initialized: bool,
}

impl Model {
    pub async fn new(ctx: Arc<MailUserContext>) -> MailboxResult<Self> {
        let stash = ctx.user_stash();
        let tether = stash.connection();
        let mailbox = Mailbox::with_remote_id(&tether, LabelId::inbox()).await?;

        ctx.prefetch().await?;

        let tether = ctx.user_stash().connection();
        let label = Label::find_by_id(mailbox.label_id(), &tether)
            .await?
            .ok_or(AppError::LabelNotFound(mailbox.label_id()))?;
        let conv_counters = ConversationCounters::find_by_id(mailbox.label_id(), &tether)
            .await?
            .ok_or(AppError::LocalLabelHasNoCounters(mailbox.label_id()))?;
        let msg_counters = MessageCounters::find_by_id(mailbox.label_id(), &tether)
            .await?
            .ok_or(AppError::LocalLabelHasNoCounters(mailbox.label_id()))?;
        let mail_settings = MailSettings::get(&tether).await?.unwrap_or_default();
        Ok(Self {
            ctx,
            mailbox,
            mail_settings: Arc::new(mail_settings),
            state: State::new_syncing(),
            label,
            conv_counters,
            msg_counters,
            cancel_token: CancellationToken::new(),
            label_watcher: None,
            composer: None,
            search: None,
            search_status: None,
            filter: ReadFilter::All,
            background_worker_initialized: false,
        })
    }

    fn create_background_worker(&mut self) -> Command<Messages> {
        let ctx = self.ctx.clone();
        if self.background_worker_initialized {
            Command::none()
        } else {
            self.background_worker_initialized = true;
            background_worker(ctx, self.cancel_token.clone())
        }
    }

    #[must_use]
    fn sync_mailbox(&mut self, mbox: Mailbox) -> Command<Messages> {
        self.state = State::new_syncing();
        let filter = self.filter;
        let ctx = Arc::clone(&self.ctx);

        // Create the background worker.
        Command::batch([
            self.create_background_worker(),
            Command::task(async move {
                let stash = ctx.user_stash();
                let tether = stash.connection();
                let label = match Label::find_by_id(mbox.label_id(), &tether).await {
                    Ok(l) => {
                        if let Some(l) = l {
                            l
                        } else {
                            let e = anyhow!(
                                "Failed to get label: {}",
                                MailboxError::LabelNotFound(mbox.label_id())
                            );
                            error!("{e:?}");
                            return Command::message(Messages::DisplayError(None, e));
                        }
                    }
                    Err(e) => {
                        let e = anyhow!("Failed to get label: {e}");
                        error!("{e:?}");
                        return Command::message(Messages::DisplayError(None, e));
                    }
                };

                if mbox.view_mode() == ViewMode::Conversations {
                    ConversationsState::build(Arc::clone(&ctx), mbox, label, filter)
                } else {
                    MessagesState::build(Arc::clone(&ctx), mbox, label, filter)
                }
            }),
        ])
    }

    fn build_item_count_query(&mut self) -> Command<Messages> {
        let label_id = self.label.local_id.unwrap();
        let stash = self.ctx.user_stash().to_owned();
        Command::task(async move {
            let label = Label::find_by_id(label_id, &stash.connection()).await;
            let handle = Label::watch(&stash);
            let label_and_recevier = label.and_then(|l| handle.map(|h| (l, h)));
            match label_and_recevier {
                Ok((label, handle)) => {
                    if let Some(label) = label {
                        let WatcherHandle {
                            handle, receiver, ..
                        } = handle;
                        let (watcher, background_command) =
                            WatchHandle::new(receiver, handle, move |()| {
                                let tether = stash.connection();
                                async move {
                                    let label_id = label.local_id.unwrap();

                                    Label::find_by_id(label_id, &tether)
                                        .await
                                        .inspect_err(|e| {
                                            tracing::error!("Failed to get label: `{e:?}`");
                                        })
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

    fn open_search_view(&mut self, mbox: Mailbox, state: MessagesState) -> Command<Messages> {
        self.mailbox = mbox;
        self.state = State::Messages(state);
        self.build_item_count_query()
    }

    fn open_label_select_popup(&mut self) -> Command<Messages> {
        let ctx = Arc::clone(&self.ctx);
        let label = self.label.clone();
        let view_mode = self.mailbox.view_mode();
        Command::task(async move {
            match LabelSelectPopup::new(ctx, &label, view_mode).await {
                Ok(state) => Command::message(Messages::RaisePopup(Box::new(state))),
                Err(e) => {
                    let e = anyhow!("Failed to load labels: {e}");
                    tracing::error!("{e:?}");
                    Command::message(Messages::DisplayError(None, e))
                }
            }
        })
    }

    fn open_move_item_popup(&mut self, item: Item) -> Command<Messages> {
        if matches!(&self.state, State::Syncing(_)) {
            return Command::None;
        }

        let ctx = Arc::clone(&self.ctx);
        Command::task(async move {
            match MoveItemPopup::new(&ctx, item).await {
                Ok(state) => Command::message(Messages::RaisePopup(Box::new(state))),
                Err(e) => {
                    let e = anyhow!("Failed to load folders: {e}");
                    tracing::error!("{e:?}");
                    Command::message(e.into())
                }
            }
        })
    }

    fn open_label_popup(&mut self, item: Item) -> Command<Messages> {
        if matches!(&self.state, State::Syncing(_)) {
            return Command::None;
        }

        let ctx = Arc::clone(&self.ctx);
        Command::task(async move {
            match LabelItemPopup::new(&ctx, item).await {
                Ok(state) => Command::message(Messages::RaisePopup(Box::new(state))),
                Err(e) => {
                    let e = anyhow!("Failed to load labels: {e}");
                    tracing::error!("{e:?}");
                    Command::message(e.into())
                }
            }
        })
    }

    fn open_contacts(&mut self) -> Command<Messages> {
        Command::message(Messages::SwitchAppState(AppState::Contacts(
            crate::app_model::contacts::Model::new(self.ctx.clone()),
        )))
    }

    fn select_label(&mut self, label_id: LocalLabelId) -> Command<Messages> {
        let ctx = Arc::clone(&self.ctx);
        Command::task(async move {
            let stash = ctx.user_stash();
            let tether = stash.connection();
            Command::message(match Mailbox::new(&tether, label_id).await {
                Ok(mbox) => Message::Sync(mbox).into(),
                Err(e) => {
                    let e = anyhow!("Failed to open label: {e}");
                    tracing::error!("{e:?}");
                    Messages::DisplayError(None, e)
                }
            })
        })
    }
}

impl AppStateHandler for Model {
    fn on_state_enter(&mut self) -> Command<Messages> {
        Command::batch([
            self.create_background_worker(),
            Command::message(Message::Sync(self.mailbox.clone()).into()),
            Command::background_task({
                let ctx = Arc::clone(&self.ctx);
                let tok = self.cancel_token.clone();
                move |sender| observe_draft_action_errors(ctx, tok, sender).boxed()
            }),
            Command::background_task({
                let ctx = Arc::clone(&self.ctx);
                let tok = self.cancel_token.clone();
                move |sender| observe_event_loop_errors(ctx, tok, sender).boxed()
            }),
        ])
    }
    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        if let Some(composer) = &mut self.composer {
            return composer.handle_event(&self.ctx, &self.mailbox, event);
        } else if let Event::Key(key) = &event {
            match key.code {
                KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Composer::empty(Arc::clone(&self.ctx));
                }
                KeyCode::Char('c') => {
                    return Command::message(Message::OpenContacts.into());
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.filter = ReadFilter::Unread;
                    return Command::message(Message::Sync(self.mailbox.clone()).into());
                }
                KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.filter = ReadFilter::Read;
                    return Command::message(Message::Sync(self.mailbox.clone()).into());
                }
                KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.filter = ReadFilter::All;
                    return Command::message(Message::Sync(self.mailbox.clone()).into());
                }
                _ => (),
            }
        }

        if let Some(search) = &mut self.search {
            return search.handle_event(&self.ctx, &self.mailbox, event);
        } else if let Event::Key(key) = &event {
            if key.code == KeyCode::Char('/') {
                return Command::Message(Message::SearchPopup(Search::new()).into());
            }
        }

        match &mut self.state {
            State::Syncing(_) => {
                // Do nothing
                Command::None
            }
            State::Conversations(state) => state.handle_event(&self.ctx, &self.mailbox, event),
            State::Messages(state) => state.handle_event(&self.ctx, &self.mailbox, event),
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
            return composer.update(ctx, &self.ctx, message, &self.mailbox, &self.mail_settings);
        }

        if let Some(search) = &mut self.search {
            let Message::CloseSearchPopup = message else {
                return search.update(ctx, &self.ctx, message, &self.mailbox, &self.mail_settings);
            };
        }

        match message {
            Message::Sync(mbox) => self.sync_mailbox(mbox),
            Message::OpenConversationView(mbox, label, state) => {
                self.open_conversation_view(mbox, label, state)
            }
            Message::OpenMessageView(mbox, label, state) => {
                self.open_message_view(mbox, label, state)
            }
            Message::OpenSearchView(mbox, state) => self.open_search_view(mbox, state),
            Message::OpenLabelSelectPopup => self.open_label_select_popup(),
            Message::SelectLabel(label_id) => self.select_label(label_id),
            Message::OpenMoveItemPopup(item) => self.open_move_item_popup(item),
            Message::OpenLabelItemPopup(item) => self.open_label_popup(item),
            Message::ConversationState(_) | Message::MessageState(_) => {
                self.state
                    .update(ctx, &self.ctx, message, &self.mailbox, &self.mail_settings)
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
            Message::OpenContacts => self.open_contacts(),
            Message::SearchPopup(search) => {
                self.search = Some(search);
                Command::None
            }
            Message::CloseSearchPopup => {
                self.search = None;
                Command::None
            }
            Message::SearchStatusBar(status) => {
                self.search_status = Some(status);
                Command::None
            }
            Message::ClearSearchStatusBar => {
                self.search_status = None;
                Command::None
            }
            Message::Composer(_) | Message::SearchSubmit(_) => Command::None,
        }
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        self.state.view(frame, area);
        if let Some(composer) = &mut self.composer {
            composer.view(frame, area);
        }
        if let Some(search) = &mut self.search {
            search.view(frame, area);
        }
    }

    fn view_help_bar(&mut self, frame: &mut Frame, area: Rect) {
        let line_1 = vec![
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
            Span::from(" Crtl+N: ").bold(),
            Span::from("New Msg."),
            Span::from(" Crtl+A: ").bold(),
            Span::from("Filter All"),
            Span::from(" Crtl+R: ").bold(),
            Span::from("Filter Read"),
            Span::from(" Crtl+U: ").bold(),
            Span::from("Filter Unread"),
            Span::from(" /: ").bold(),
            Span::from("Search"),
        ];
        let line_2 = vec![
            Span::from(" Shift+▲: ").bold(),
            Span::from("Msg. Up"),
            Span::from(" Shift+▼: ").bold(),
            Span::from("Msg. Down"),
            Span::from(" Crtl+R: ").bold(),
            Span::from("Reply Msg."),
            Span::from(" Crtl+T: ").bold(),
            Span::from("Reply All Msg."),
            Span::from(" Crtl+F: ").bold(),
            Span::from("Forward Msg."),
            Span::from(" a ").bold(),
            Span::from("Fetch all atts."),
            Span::from(" C: ").bold(),
            Span::from("Contacts"),
        ];

        let [one, two] =
            Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);
        frame.render_widget(Line::from(line_1), one);
        frame.render_widget(Line::from(line_2), two);
    }

    fn help_bar_lines(&self) -> u16 {
        2
    }

    fn view_status_bar(&mut self, frame: &mut Frame, area: Rect) {
        if let Some(ref status) = self.search_status {
            let [count_area, _, other_area] = Layout::horizontal([
                Constraint::Length(16),
                Constraint::Length(1),
                Constraint::Percentage(100),
            ])
            .flex(Flex::Start)
            .areas(area);

            let count = format!("Search T:{total:4}", total = status.total);
            frame.render_widget(Text::from(count), count_area);

            let search = format!("phrase: `{}`, press ESC to go back.", status.search_phrase);
            frame.render_widget(Text::from(search), other_area);
        } else {
            let label_name = self
                .label
                .path
                .as_deref()
                .unwrap_or(self.label.name.as_str());

            let (total, unread) = if self.mailbox.view_mode() == ViewMode::Conversations {
                (self.conv_counters.total, self.conv_counters.unread)
            } else {
                (self.msg_counters.total, self.msg_counters.unread)
            };
            let counters = format!("T:{total:4} U:{unread:4}");
            let [label_area, _, count_area, filter_area, other_area] = Layout::horizontal([
                Constraint::Length(u16::try_from(label_name.chars().count()).unwrap_or(10)),
                Constraint::Length(1),
                Constraint::Length(13),
                Constraint::Length(13),
                Constraint::Percentage(100),
            ])
            .flex(Flex::Start)
            .areas(area);

            let text = Text::from(label_name);
            frame.render_widget(text, label_area);
            frame.render_widget(Text::from(counters), count_area);
            frame.render_widget(
                Text::from(format!(" | {:?} | ", self.filter).bold()),
                filter_area,
            );
            if let State::Conversations(state) = &mut self.state {
                state.draw_status_bar(frame, other_area);
            }
        }
    }
}

impl StateHandler for State {
    fn handle_event(
        &mut self,
        ctx: &Arc<MailUserContext>,
        mbox: &Mailbox,
        event: Event,
    ) -> Command<Messages> {
        match self {
            State::Syncing(_) => Command::None,
            State::Conversations(state) => state.handle_event(ctx, mbox, event),
            State::Messages(state) => state.handle_event(ctx, mbox, event),
        }
    }

    fn update(
        &mut self,
        ctx: &MailContext,
        user_ctx: &Arc<MailUserContext>,
        message: Message,
        mbox: &Mailbox,
        mail_settings: &Arc<MailSettings>,
    ) -> Command<Messages> {
        match self {
            State::Syncing(_) => Command::None,

            State::Conversations(state) => {
                state.update(ctx, user_ctx, message, mbox, mail_settings)
            }

            State::Messages(state) => state.update(ctx, user_ctx, message, mbox, mail_settings),
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

fn background_worker(
    context: Arc<MailUserContext>,
    cancel_token: CancellationToken,
) -> Command<Messages> {
    Command::background_task(|sender| {
        async move {
            let event_loop_poll_time = CLI_ARGS.event_loop_time.unwrap_or(15);
            let mut interval = tokio::time::interval(Duration::from_secs(event_loop_poll_time));
            loop {
                tokio::select! {
                () = cancel_token.cancelled() => {
                    return;
                }
                _ = interval.tick() => {
                        if let Err(e) = context.poll_event_loop().await {
                            let e = anyhow!("Failed to poll events: {e}");
                            error!("{e:?}");
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

/// Observe and report draft save failures.
async fn observe_draft_action_errors(
    ctx: Arc<MailUserContext>,
    cancellation_token: CancellationToken,
    sender: Sender<Command<Messages>>,
) {
    let mut observer = match DraftSendResultWatcher::new(ctx.user_stash().clone()).await {
        Ok(observer) => observer,
        Err(e) => {
            error!("Failed to create draft send result observer:{e:?}");
            let _ = sender
                .send_async(Command::message(Messages::DisplayError(
                    Some("Draft Send Result".to_owned()),
                    anyhow::Error::new(e),
                )))
                .await;
            return;
        }
    };

    loop {
        tokio::select! {
            () = cancellation_token.cancelled() => {
                debug!(
                    "Exiting draft save observer"
                );
                return;
            }
            r = observer.next() => {
                match r {
                    Ok(failures) => {
                        handle_draft_failure(&ctx, &sender, failures).await;
                    }
                    Err(e) => {
                        error!("Failed to observe: {e:?}");
                        return;
                    }
                }
            }

        }
    }
}

async fn handle_draft_failure(
    ctx: &Arc<MailUserContext>,
    sender: &Sender<Command<Messages>>,
    results: Vec<DraftSendResult>,
) {
    for result in results {
        if result.is_success() {
            if result.is_send_undoable() {
                let ctx = Arc::clone(ctx);
                let popup = YesNoPopup::new(
                    "Undo Send?",
                    "Message was sent successfully, would you like to undo this send?",
                )
                .on_accept(Command::batch([
                    Command::message(Messages::DisplayBackgroundProgress(
                        "Cancelling Send".to_owned(),
                    )),
                    Command::task(async move {
                        let result_cmd = match Draft::action_undo_send(
                            ctx.action_queue(),
                            result.local_message_id,
                        )
                        .await
                        {
                            // On success open composer, else display error
                            Ok(_) => Composer::open(Arc::clone(&ctx), result.local_message_id),
                            Err(e) => Command::message(Messages::DisplayError(
                                Some("Undo Send Error".to_owned()),
                                anyhow::Error::new(e),
                            )),
                        };
                        Command::batch([
                            Command::message(Messages::DismissBackgroundProgress),
                            result_cmd,
                        ])
                    }),
                ]));

                let _ = sender
                    .send_async(Command::message(Messages::raise_popup(popup)))
                    .await;
            }

            continue;
        }

        if result.origin == DraftSendResultOrigin::Save {
            let _ = sender
                .send_async(Command::message(Messages::DisplayError(
                    Some("Failed to Save Draft".to_string()),
                    anyhow!("{:?}", result.error.unwrap_or(DraftSendFailure::Internal)),
                )))
                .await;
        } else {
            let err_command = Command::message(Messages::DisplayError(
                Some("Failed to Send Draft".to_string()),
                anyhow!("{:?}", result.error.unwrap_or(DraftSendFailure::Internal)),
            ));

            let open_composer_command = Composer::open(Arc::clone(ctx), result.local_message_id);
            let _ = sender
                .send_async(Command::batch([err_command, open_composer_command]))
                .await;
        }
    }
}

/// Observe event loop errors
async fn observe_event_loop_errors(
    ctx: Arc<MailUserContext>,
    cancellation_token: CancellationToken,
    sender: Sender<Command<Messages>>,
) {
    let mut oberserver = ActionFailureObserver::<EventPoll>::new(ctx.action_queue());

    loop {
        select! {
            () = cancellation_token.cancelled() => {
                return;
            }

            r = oberserver.next() => {
                let Ok(v) = r else {
                    return;
                };
                handle_event_loop_error(v, &sender).await;
            }
        }
    }
}

// Convert the error into a user displayable message.
async fn handle_event_loop_error(error: ActionFailureReason, sender: &Sender<Command<Messages>>) {
    if let ActionFailureReason::Error(e, _) = error {
        let cmd = if let Some(details) = e.as_action_error::<EventPoll>() {
            match details {
                ActionError::Action(e) => Command::message(Messages::DisplayError(
                    Some("Event Loop".to_owned()),
                    anyhow!("Event Poll Failure: {}", e),
                )),
                ActionError::Queue(e) => Command::message(Messages::DisplayError(
                    Some("Event Loop".to_owned()),
                    anyhow!("Queue Failure: {}", e),
                )),
            }
        } else {
            Command::message(Messages::DisplayError(
                Some("Event Loop".to_owned()),
                anyhow!("Failed to poll the event loop"),
            ))
        };
        let _ = sender.send_async(cmd).await;
    }
}
