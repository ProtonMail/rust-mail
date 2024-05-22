use crate::app_model::mailbox::conversations::ConversationsState;
use crate::app_model::mailbox::messages::MessagesState;
use crate::app_model::mailbox::popups::{LabelItemPopup, LabelSelectPopup, MoveItemPopup};
use crate::app_model::mailbox::{Item, Message, ITEM_LIMIT};
use crate::app_model::{AppState, AppStateHandler, BackgroundSender};
use crate::messages::Messages;
use anyhow::anyhow;
use crossterm::event::Event;
use proton_api_mail::domain::{LabelId, MailSettingsViewMode};
use proton_api_mail::exports::tracing;
use proton_mail_common::db::{LocalConversationId, LocalLabelId};
use proton_mail_common::{MailContext, MailUserContext, Mailbox, MailboxResult};
use ratatui::layout::{Constraint, Flex, Layout, Rect};
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
pub struct Model {
    ctx: MailUserContext,
    mailbox: Mailbox,
    state: State,
    cancel_token: Option<Sender<()>>,
}

impl Model {
    pub fn new(ctx: MailUserContext) -> MailboxResult<Self> {
        let mailbox = Mailbox::with_remote_id(ctx.clone(), LabelId::inbox())?;

        Ok(Self {
            ctx,
            mailbox,
            state: State::new_syncing(),
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
            if let Err(e) = mbox.sync(ITEM_LIMIT).await {
                let e = anyhow!("Failed to sync mailbox: {e}");
                tracing::error!("{e}");
                sender.send(Messages::DisplayError(None, e));
            };

            let msg = if mbox.view_mode() == MailSettingsViewMode::Conversations {
                Message::OpenConversationView(mbox)
            } else {
                Message::OpenMessageView(mbox)
            };

            sender.send(msg.into());
        });
    }

    fn open_conversation_view(&mut self, mbox: Mailbox) -> Option<Messages> {
        self.mailbox = mbox;
        match ConversationsState::new(&self.mailbox) {
            Ok(state) => {
                self.state = State::Conversations(state);
                None
            }
            Err(e) => Some(Messages::from(e)),
        }
    }

    fn open_message_view(&mut self, mbox: Mailbox) -> Option<Messages> {
        self.mailbox = mbox;
        match MessagesState::new(&self.mailbox) {
            Ok(state) => {
                self.state = State::Messages(state);
                None
            }
            Err(e) => Some(Messages::from(e)),
        }
    }

    fn open_label_select_popup(&mut self) -> Option<Messages> {
        let label = match &self.state {
            State::Syncing(_) => return None,
            State::Conversations(state) => state.label(),
            State::Messages(state) => state.label(),
        };
        match LabelSelectPopup::new(self.mailbox.user_context(), label) {
            Ok(state) => Some(Messages::RaisePopup(Box::new(state))),
            Err(e) => {
                let e = anyhow!("Failed to load labels: {e}");
                tracing::error!("{e}");
                Some(Messages::DisplayError(None, e))
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

    fn mark_conversation_read(&mut self, id: LocalConversationId) -> Option<Messages> {
        if !matches!(&self.state, State::Conversations(_)) {
            return None;
        }
        match self.mailbox.mark_conversations_read(std::iter::once(id)) {
            Ok(()) => None,
            Err(e) => {
                let e = anyhow!("Failed to mark conversation as read: {e}");
                tracing::error!("{e}");
                Some(e.into())
            }
        }
    }

    fn mark_conversation_unread(&mut self, id: LocalConversationId) -> Option<Messages> {
        if !matches!(&self.state, State::Conversations(_)) {
            return None;
        }
        match self.mailbox.mark_conversations_unread(std::iter::once(id)) {
            Ok(()) => None,
            Err(e) => {
                let e = anyhow!("Failed to mark conversation as read: {e}");
                tracing::error!("{e}");
                Some(e.into())
            }
        }
    }

    fn delete_conversation(&mut self, id: LocalConversationId) -> Option<Messages> {
        if !matches!(&self.state, State::Conversations(_)) {
            return None;
        }
        match self.mailbox.delete_conversations(std::iter::once(id)) {
            Ok(()) => None,
            Err(e) => {
                let e = anyhow!("Failed to delete conversation: {e}");
                tracing::error!("{e}");
                Some(e.into())
            }
        }
    }

    fn move_conversation(
        &mut self,
        conversation_id: LocalConversationId,
        label_id: LocalLabelId,
    ) -> Messages {
        match self
            .mailbox
            .move_conversations(label_id, std::iter::once(conversation_id))
        {
            Ok(()) => Messages::DismissPopup,
            Err(e) => {
                let e = anyhow!("Failed to move conversation: {e}");
                tracing::error!("{e}");
                e.into()
            }
        }
    }

    fn label_conversation(
        &mut self,
        conversation_id: LocalConversationId,
        label_id: LocalLabelId,
    ) -> Messages {
        match self
            .mailbox
            .label_conversations(label_id, std::iter::once(conversation_id))
        {
            Ok(()) => Messages::DismissPopup,
            Err(e) => {
                let e = anyhow!("Failed to label conversation: {e}");
                tracing::error!("{e}");
                e.into()
            }
        }
    }

    fn unlabel_conversation(
        &mut self,
        conversation_id: LocalConversationId,
        label_id: LocalLabelId,
    ) -> Messages {
        match self
            .mailbox
            .unlabel_conversations(label_id, std::iter::once(conversation_id))
        {
            Ok(()) => Messages::DismissPopup,
            Err(e) => {
                let e = anyhow!("Failed to unlabel conversation: {e}");
                tracing::error!("{e}");
                e.into()
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
            State::Conversations(state) => state.on_event(&event),
            State::Messages(state) => state.on_event(&event),
        }
    }

    fn update(
        &mut self,
        _: &MailContext,
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
            Message::OpenConversationView(mbox) => self.open_conversation_view(mbox),
            Message::OpenMessageView(mbox) => self.open_message_view(mbox),
            Message::OpenLabelSelectPopup => self.open_label_select_popup(),
            Message::SelectLabel(label_id) => Some(self.select_label(label_id)),
            Message::MarkConversationRead(id) => self.mark_conversation_read(id),
            Message::MarkConversationUnread(id) => self.mark_conversation_unread(id),
            Message::DeleteConversation(id) => self.delete_conversation(id),
            Message::OpenMoveItemPopup(item) => self.open_move_item_popup(item),
            Message::MoveConversation(conversation_id, label_id) => {
                Some(self.move_conversation(conversation_id, label_id))
            }
            Message::LabelConversation(conversation_id, label_id) => {
                Some(self.label_conversation(conversation_id, label_id))
            }
            Message::UnlabelConversation(conversation_id, label_id) => {
                Some(self.unlabel_conversation(conversation_id, label_id))
            }
            Message::OpenLabelItemPopup(item) => self.open_label_popup(item),
            Message::OpenUnlabelItemPopup(item) => self.open_unlabel_popup(item),
        }
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        match &mut self.state {
            State::Syncing(state) => {
                let [_, content, _] = Layout::vertical([
                    Constraint::Percentage(50),
                    Constraint::Min(1),
                    Constraint::Percentage(50),
                ])
                .flex(Flex::SpaceAround)
                .areas(area);
                let [_, content, _] = Layout::horizontal([
                    Constraint::Percentage(50),
                    Constraint::Min(10),
                    Constraint::Percentage(50),
                ])
                .flex(Flex::SpaceAround)
                .areas(content);

                state.calc_next();
                let full = throbber_widgets_tui::Throbber::default()
                    .label("Loading...")
                    .throbber_set(throbber_widgets_tui::BRAILLE_SIX)
                    .use_type(throbber_widgets_tui::WhichUse::Spin);
                frame.render_stateful_widget(full, content, state);
            }
            State::Conversations(state) => {
                state.draw(frame, area);
            }
            State::Messages(state) => {
                state.draw(frame, area);
            }
        }
    }
    fn view_status_bar(&mut self, frame: &mut Frame, area: Rect) {
        match &mut self.state {
            State::Syncing(_) => {}
            State::Conversations(state) => {
                state.draw_status_bar(frame, area);
            }
            State::Messages(state) => {
                state.draw_status_bar(frame, area);
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
