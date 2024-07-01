use crate::app_model::mailbox::conversations::ConversationsState;
use crate::app_model::mailbox::messages::MessagesState;
use crate::app_model::mailbox::popups::{LabelItemPopup, LabelSelectPopup, MoveItemPopup};
use crate::app_model::mailbox::{Item, LiveQueryBuilder, Message, ITEM_LIMIT};
use crate::app_model::{AppState, AppStateHandler, BackgroundSender};
use crate::messages::Messages;
use crate::widgets::CenteredThrobber;
use anyhow::anyhow;
use crossterm::event::Event;
use proton_core_common::db::proton_sqlite3::Observed;
use proton_core_common::db::DBResult;
use proton_mail_common::db::{LabelItemCount, LocalLabel, LocalLabelId};
use proton_mail_common::exports::tracing;
use proton_mail_common::proton_api_mail::domain::{LabelId, MailSettingsViewMode};
use proton_mail_common::{MailContext, MailUserContext, Mailbox, MailboxError, MailboxResult};
use ratatui::layout::{Flex, Rect};
use ratatui::prelude::*;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
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
    fn handle_event(&mut self, event: Event) -> Option<Messages>;

    fn update(
        &mut self,
        ctx: &MailContext,
        message: Message,
        mbox: &Mailbox,
        sender: &BackgroundSender,
    ) -> Option<Messages>;
    fn view(&mut self, frame: &mut Frame, area: Rect);
}
pub struct Model {
    ctx: MailUserContext,
    mailbox: Mailbox,
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
        Ok(Self {
            ctx,
            mailbox,
            state: State::new_syncing(),
            label,
            item_count_query: None,
            item_count: None,
            cancel_token: None,
        })
    }

    fn init_background_worker(&mut self, background_sender: &BackgroundSender) {
        if self.cancel_token.is_some() {
            return;
        }

        let ctx = self.ctx.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        self.cancel_token = Some(sender);
        let background_sender = background_sender.clone();
        std::thread::spawn(move || {
            background_worker(&ctx, &receiver, &background_sender);
        });
    }

    fn sync_mailbox(&mut self, mbox: Mailbox, sender: BackgroundSender) {
        self.state = State::new_syncing();
        // Create the background worker.
        self.init_background_worker(&sender);
        self.ctx.mail_context().async_runtime().spawn(async move {
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
                        sender.send(Messages::DisplayError(None, e));
                        return;
                    }
                }
                Err(e) => {
                    let e = anyhow!("Failed to get label: {e}");
                    tracing::error!("{e}");
                    sender.send(Messages::DisplayError(None, e));
                    return;
                }
            };
            if let Err(e) = mbox.sync(ITEM_LIMIT).await {
                let e = anyhow!("Failed to sync mailbox: {e}");
                tracing::error!("{e}");
                sender.send(Messages::DisplayError(None, e));
                return;
            };

            let msg = if mbox.view_mode() == MailSettingsViewMode::Conversations {
                Message::OpenConversationView(mbox, label)
            } else {
                Message::OpenMessageView(mbox, label)
            };

            sender.send(msg.into());
        });
    }

    fn build_item_count_query(&mut self, background_sender: BackgroundSender) -> Option<Messages> {
        self.item_count_query = None;
        self.item_count = None;
        let local_label = match self
            .mailbox
            .user_context()
            .db_read(|conn| conn.label_with_id_and_conversation_count(self.mailbox.label_id()))
        {
            Ok(label) => label,
            Err(e) => return Some(MailboxError::from(e).into()),
        };
        self.item_count = local_label.map(|l| LabelItemCount {
            unread: l.unread_count,
            total: l.total_count,
        });
        match self
            .mailbox
            .new_label_item_count_query(LiveQueryBuilder::new(
                label_item_count_converter,
                background_sender,
            )) {
            Ok(q) => {
                self.item_count_query = Some(q);
                None
            }
            Err(e) => Some(e.into()),
        }
    }

    fn open_conversation_view(
        &mut self,
        mbox: Mailbox,
        label: LocalLabel,
        background_sender: BackgroundSender,
    ) -> Option<Messages> {
        self.mailbox = mbox;
        match ConversationsState::new(&self.mailbox, background_sender.clone()) {
            Ok(state) => {
                self.state = State::Conversations(state);
                self.label = label;
                self.build_item_count_query(background_sender)
            }
            Err(e) => Some(Messages::from(e)),
        }
    }

    fn open_message_view(
        &mut self,
        mbox: Mailbox,
        label: LocalLabel,
        background_sender: BackgroundSender,
    ) -> Option<Messages> {
        self.mailbox = mbox;
        let state = match MessagesState::new(&self.mailbox, background_sender.clone()) {
            Ok(state) => state,
            Err(e) => {
                return Some(e.into());
            }
        };
        self.label = label;
        self.state = State::Messages(state);
        self.build_item_count_query(background_sender)
    }

    fn item_count_refreshed(&mut self, item_count: LabelItemCount) {
        self.item_count = Some(item_count);
    }

    fn open_label_select_popup(&mut self) -> Messages {
        match LabelSelectPopup::new(self.mailbox.user_context(), &self.label) {
            Ok(state) => Messages::RaisePopup(Box::new(state)),
            Err(e) => {
                let e = anyhow!("Failed to load labels: {e}");
                tracing::error!("{e}");
                Messages::DisplayError(None, e)
            }
        }
    }

    fn open_move_item_popup(&mut self, item: Item) -> Option<Messages> {
        if matches!(&self.state, State::Syncing(_)) {
            return None;
        };

        match MoveItemPopup::new(&self.ctx, item) {
            Ok(state) => Some(Messages::RaisePopup(Box::new(state))),
            Err(e) => {
                let e = anyhow!("Failed to load folders: {e}");
                tracing::error!("{e}");
                Some(e.into())
            }
        }
    }

    fn open_label_popup(&mut self, item: Item) -> Option<Messages> {
        if matches!(&self.state, State::Syncing(_)) {
            return None;
        };

        match LabelItemPopup::new(&self.ctx, item, true) {
            Ok(state) => Some(Messages::RaisePopup(Box::new(state))),
            Err(e) => {
                let e = anyhow!("Failed to load labels: {e}");
                tracing::error!("{e}");
                Some(e.into())
            }
        }
    }

    fn open_unlabel_popup(&mut self, item: Item) -> Option<Messages> {
        if matches!(&self.state, State::Syncing(_)) {
            return None;
        };

        match LabelItemPopup::new(&self.ctx, item, false) {
            Ok(state) => Some(Messages::RaisePopup(Box::new(state))),
            Err(e) => {
                let e = anyhow!("Failed to load labels: {e}");
                tracing::error!("{e}");
                Some(e.into())
            }
        }
    }

    fn select_label(&mut self, label_id: LocalLabelId) -> Messages {
        match Mailbox::with_id(self.ctx.clone(), label_id) {
            Ok(mbox) => Message::Sync(mbox).into(),
            Err(e) => {
                let e = anyhow!("Failed to open label: {e}");
                tracing::error!("{e}");
                Messages::DisplayError(None, e)
            }
        }
    }
}

impl AppStateHandler for Model {
    fn on_state_enter(&mut self) -> Option<Messages> {
        Some(Message::Sync(self.mailbox.clone()).into())
    }
    fn handle_event(&mut self, event: Event) -> Option<Messages> {
        match &mut self.state {
            State::Syncing(_) => {
                // Do nothing
                None
            }
            State::Conversations(state) => state.handle_event(event),
            State::Messages(state) => state.handle_event(event),
        }
    }

    fn update(
        &mut self,
        ctx: &MailContext,
        message: Messages,
        sender: &BackgroundSender,
    ) -> Option<Messages> {
        let Messages::Mailbox(message) = message else {
            return None;
        };

        match message {
            Message::Sync(mbox) => {
                self.sync_mailbox(mbox, sender.clone());
                None
            }
            Message::OpenConversationView(mbox, label) => {
                self.open_conversation_view(mbox, label, sender.clone())
            }
            Message::OpenMessageView(mbox, label) => {
                self.open_message_view(mbox, label, sender.clone())
            }
            Message::OpenLabelSelectPopup => Some(self.open_label_select_popup()),
            Message::SelectLabel(label_id) => Some(self.select_label(label_id)),
            Message::OpenMoveItemPopup(item) => self.open_move_item_popup(item),
            Message::OpenLabelItemPopup(item) => self.open_label_popup(item),
            Message::OpenUnlabelItemPopup(item) => self.open_unlabel_popup(item),
            Message::ConversationState(_) | Message::MessageState(_) => {
                self.state.update(ctx, message, &self.mailbox, sender)
            }
            Message::ItemCountRefreshed(count) => {
                self.item_count_refreshed(count);
                None
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
    fn handle_event(&mut self, event: Event) -> Option<Messages> {
        match self {
            State::Syncing(_) => None,
            State::Conversations(state) => state.handle_event(event),
            State::Messages(state) => state.handle_event(event),
        }
    }

    fn update(
        &mut self,
        ctx: &MailContext,
        message: Message,
        mbox: &Mailbox,
        sender: &BackgroundSender,
    ) -> Option<Messages> {
        match self {
            State::Syncing(_) => None,
            State::Conversations(state) => state.update(ctx, message, mbox, sender),
            State::Messages(state) => state.update(ctx, message, mbox, sender),
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
    context: &MailUserContext,
    reader: &Receiver<()>,
    background_sender: &BackgroundSender,
) {
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
            background_sender.send(Messages::DisplayError(Some("Action Queue".to_owned()), e));
        }

        if let Err(e) = context
            .mail_context()
            .async_runtime()
            .block_on(async { context.poll_event_loop().await })
        {
            let e = anyhow!("Failed to poll events: {e}");
            tracing::error!("{e}");
            background_sender.send(Messages::DisplayError(Some("Event Loop".to_owned()), e));
        }
    }
}

fn label_item_count_converter(value: DBResult<LabelItemCount>) -> Messages {
    match value {
        Ok(value) => Message::ItemCountRefreshed(value).into(),
        Err(e) => {
            let e = anyhow!("Label Item Count Query error: {e}");
            tracing::error!("{e}");
            e.into()
        }
    }
}
