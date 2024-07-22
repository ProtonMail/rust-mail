use crate::app::Command;
use crate::app_model::mailbox::conversations::ConversationsState;
use crate::app_model::mailbox::messages::MessagesState;
use crate::app_model::mailbox::popups::{LabelItemPopup, LabelSelectPopup, MoveItemPopup};
use crate::app_model::mailbox::{Item, LiveQueryBuilder, Message, ITEM_LIMIT};
use crate::app_model::{AppState, AppStateHandler};
use crate::messages::Messages;
use crate::widgets::CenteredThrobber;
use anyhow::anyhow;
use crossterm::event::Event;
use proton_core_common::db::proton_sqlite3::Observed;
use proton_core_common::db::DBResult;
use proton_mail_common::db::{LabelItemCount, LocalLabel, LocalLabelId};
use proton_mail_common::exports::tracing;
use proton_mail_common::proton_api_mail::domain::{LabelId, MailSettingsViewMode};
use proton_mail_common::settings::MailSettings;
use proton_mail_common::{MailContext, MailUserContext, Mailbox, MailboxError, MailboxResult};
use ratatui::layout::{Flex, Rect};
use ratatui::prelude::*;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::Duration;
use throbber_widgets_tui::ThrobberState;

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
    fn handle_event(&mut self, event: Event) -> Command<Messages>;

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
    ctx: MailUserContext,
    mailbox: Mailbox,
    mail_settings: Arc<MailSettings>,
    label: LocalLabel,
    item_count_query: Option<Observed>,
    item_count: Option<LabelItemCount>,
    state: State,
    cancel_token: Option<Sender<()>>,
}

impl Model {
    pub fn new(ctx: MailUserContext) -> MailboxResult<Self> {
        let mailbox = Mailbox::with_remote_id(ctx.clone(), LabelId::inbox())?;
        let label = ctx
            .get_label_with_remote_id(LabelId::inbox())?
            .ok_or(MailboxError::RemoteLabelNotFound(LabelId::inbox().clone()))?;
        let mail_settings = MailSettings::new(&ctx, None);
        Ok(Self {
            ctx,
            mailbox,
            mail_settings: Arc::new(mail_settings),
            state: State::new_syncing(),
            label,
            item_count_query: None,
            item_count: None,
            cancel_token: None,
        })
    }

    fn init_background_worker(&mut self) {
        if self.cancel_token.is_some() {
            return;
        }

        let ctx = self.ctx.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        self.cancel_token = Some(sender);
        std::thread::spawn(move || {
            background_worker(&ctx, &receiver);
        });
    }

    #[must_use]
    fn sync_mailbox(&mut self, mbox: Mailbox) -> Command<Messages> {
        self.state = State::new_syncing();
        // Create the background worker.
        self.init_background_worker();

        Command::task(async {
            let label = match mbox.user_context().get_label(mbox.label_id()) {
                Ok(l) => {
                    if let Some(l) = l {
                        l
                    } else {
                        let e = anyhow!(
                            "Failed to get label: {}",
                            MailboxError::LabelNotFound(mbox.label_id())
                        );
                        tracing::error!("{e}");
                        return Command::message(Messages::DisplayError(None, e));
                    }
                }
                Err(e) => {
                    let e = anyhow!("Failed to get label: {e}");
                    tracing::error!("{e}");
                    return Command::message(Messages::DisplayError(None, e));
                }
            };
            if let Err(e) = mbox.sync(ITEM_LIMIT).await {
                let e = anyhow!("Failed to sync mailbox: {e}");
                tracing::error!("{e}");
                return Command::message(Messages::DisplayError(None, e));
            };

            let msg = if mbox.view_mode() == MailSettingsViewMode::Conversations {
                Message::OpenConversationView(mbox, label)
            } else {
                Message::OpenMessageView(mbox, label)
            };

            Command::message(msg.into())
        })
    }

    fn build_item_count_query(&mut self) -> Command<Messages> {
        self.item_count_query = None;
        self.item_count = None;
        let local_label = match self
            .mailbox
            .user_context()
            .db_read(|conn| conn.label_with_id_and_conversation_count(self.mailbox.label_id()))
        {
            Ok(label) => label,
            Err(e) => return Command::message(MailboxError::from(e).into()),
        };
        self.item_count = local_label.map(|l| LabelItemCount {
            unread: l.unread_count,
            total: l.total_count,
        });
        match self
            .mailbox
            .new_label_item_count_query(LiveQueryBuilder::new(label_item_count_converter))
        {
            Ok(q) => {
                self.item_count_query = Some(q);
                Command::None
            }
            Err(e) => Command::message(e.into()),
        }
    }

    fn open_conversation_view(&mut self, mbox: Mailbox, label: LocalLabel) -> Command<Messages> {
        self.mailbox = mbox;
        match ConversationsState::new(&self.mailbox) {
            Ok(state) => {
                self.state = State::Conversations(state);
                self.label = label;
                self.build_item_count_query()
            }
            Err(e) => Command::message(Messages::from(e)),
        }
    }

    fn open_message_view(&mut self, mbox: Mailbox, label: LocalLabel) -> Command<Messages> {
        self.mailbox = mbox;
        let state = match MessagesState::new(&self.mailbox) {
            Ok(state) => state,
            Err(e) => {
                return Command::message(e.into());
            }
        };
        self.label = label;
        self.state = State::Messages(state);
        self.build_item_count_query()
    }

    fn item_count_refreshed(&mut self, item_count: LabelItemCount) {
        self.item_count = Some(item_count);
    }

    fn open_label_select_popup(&mut self) -> Command<Messages> {
        match LabelSelectPopup::new(self.mailbox.user_context(), &self.label) {
            Ok(state) => Command::message(Messages::RaisePopup(Box::new(state))),
            Err(e) => {
                let e = anyhow!("Failed to load labels: {e}");
                tracing::error!("{e}");
                Command::message(Messages::DisplayError(None, e))
            }
        }
    }

    fn open_move_item_popup(&mut self, item: Item) -> Command<Messages> {
        if matches!(&self.state, State::Syncing(_)) {
            return Command::None;
        };

        match MoveItemPopup::new(&self.ctx, item) {
            Ok(state) => Command::message(Messages::RaisePopup(Box::new(state))),
            Err(e) => {
                let e = anyhow!("Failed to load folders: {e}");
                tracing::error!("{e}");
                Command::message(e.into())
            }
        }
    }

    fn open_label_popup(&mut self, item: Item) -> Command<Messages> {
        if matches!(&self.state, State::Syncing(_)) {
            return Command::None;
        };

        match LabelItemPopup::new(&self.ctx, item, true) {
            Ok(state) => Command::message(Messages::RaisePopup(Box::new(state))),
            Err(e) => {
                let e = anyhow!("Failed to load labels: {e}");
                tracing::error!("{e}");
                Command::message(e.into())
            }
        }
    }

    fn open_unlabel_popup(&mut self, item: Item) -> Command<Messages> {
        if matches!(&self.state, State::Syncing(_)) {
            return Command::None;
        };

        match LabelItemPopup::new(&self.ctx, item, false) {
            Ok(state) => Command::message(Messages::RaisePopup(Box::new(state))),
            Err(e) => {
                let e = anyhow!("Failed to load labels: {e}");
                tracing::error!("{e}");
                Command::message(e.into())
            }
        }
    }

    fn select_label(&mut self, label_id: LocalLabelId) -> Command<Messages> {
        Command::message(match Mailbox::with_id(self.ctx.clone(), label_id) {
            Ok(mbox) => Message::Sync(mbox).into(),
            Err(e) => {
                let e = anyhow!("Failed to open label: {e}");
                tracing::error!("{e}");
                Messages::DisplayError(None, e)
            }
        })
    }
}

impl AppStateHandler for Model {
    fn on_state_enter(&mut self) -> Command<Messages> {
        Command::message(Message::Sync(self.mailbox.clone()).into())
    }
    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        match &mut self.state {
            State::Syncing(_) => {
                // Do nothing
                Command::None
            }
            State::Conversations(state) => state.handle_event(event),
            State::Messages(state) => state.handle_event(event),
        }
    }

    fn update(&mut self, ctx: &MailContext, message: Messages) -> Command<Messages> {
        let Messages::Mailbox(message) = message else {
            return Command::None;
        };

        match message {
            Message::Sync(mbox) => self.sync_mailbox(mbox),
            Message::OpenConversationView(mbox, label) => self.open_conversation_view(mbox, label),
            Message::OpenMessageView(mbox, label) => self.open_message_view(mbox, label),
            Message::OpenLabelSelectPopup => self.open_label_select_popup(),
            Message::SelectLabel(label_id) => self.select_label(label_id),
            Message::OpenMoveItemPopup(item) => self.open_move_item_popup(item),
            Message::OpenLabelItemPopup(item) => self.open_label_popup(item),
            Message::OpenUnlabelItemPopup(item) => self.open_unlabel_popup(item),
            Message::ConversationState(_) | Message::MessageState(_) => {
                self.state
                    .update(ctx, message, &self.mailbox, &self.mail_settings)
            }
            Message::ItemCountRefreshed(count) => {
                self.item_count_refreshed(count);
                Command::None
            }
        }
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        self.state.view(frame, area);
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
            Span::from(" L: ").bold(),
            Span::from("Label"),
            Span::from(" K: ").bold(),
            Span::from("Unlabel"),
            Span::from(" D: ").bold(),
            Span::from("Delete"),
            Span::from(" Shift+▲: ").bold(),
            Span::from("Msg. Up"),
            Span::from(" Shift+▼: ").bold(),
            Span::from("Msg. Down"),
        ];
        frame.render_widget(Line::from(spans), area);
    }

    fn view_status_bar(&mut self, frame: &mut Frame, area: Rect) {
        let label_name = self
            .label
            .path
            .as_deref()
            .unwrap_or(self.label.name.as_str());

        let counters = self
            .item_count
            .as_ref()
            .map(|counts| format!("T:{:4} U:{:4}", counts.total, counts.unread));
        let [label_area, _, count_area, other_area] = Layout::horizontal([
            Constraint::Length(u16::try_from(label_name.chars().count()).unwrap_or(10)),
            Constraint::Length(1),
            if counters.is_some() {
                Constraint::Length(13)
            } else {
                Constraint::Length(1)
            },
            Constraint::Percentage(100),
        ])
        .flex(Flex::Start)
        .areas(area);
        let text = Text::from(label_name);
        frame.render_widget(text, label_area);
        if let Some(counters) = counters {
            frame.render_widget(Text::from(counters), count_area);
        }
        if let State::Conversations(state) = &mut self.state {
            state.draw_status_bar(frame, other_area);
        }
    }
}

impl StateHandler for State {
    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        match self {
            State::Syncing(_) => Command::None,
            State::Conversations(state) => state.handle_event(event),
            State::Messages(state) => state.handle_event(event),
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

fn background_worker(context: &MailUserContext, reader: &Receiver<()>) {
    let interval = Duration::from_secs(15);
    loop {
        if let Err(e) = reader.recv_timeout(interval) {
            if e == RecvTimeoutError::Disconnected {
                return;
            }
        }
        if let Err(e) = context.execute_pending_actions() {
            let e = anyhow!("Failed to flush actions: {e}");
            tracing::error!("{e}");
            send_background(Command::message(Messages::DisplayError(
                Some("Action Queue".to_owned()),
                e,
            )));
        }

        if let Err(e) = context
            .mail_context()
            .async_runtime()
            .block_on(async { context.poll_event_loop().await })
        {
            let e = anyhow!("Failed to poll events: {e}");
            tracing::error!("{e}");
            send_background(Command::message(Messages::DisplayError(
                Some("Event Loop".to_owned()),
                e,
            )));
        }
    }
}

fn label_item_count_converter(value: DBResult<LabelItemCount>) -> Command<Messages> {
    Command::message(match value {
        Ok(value) => Message::ItemCountRefreshed(value).into(),
        Err(e) => {
            let e = anyhow!("Label Item Count Query error: {e}");
            tracing::error!("{e}");
            e.into()
        }
    })
}
