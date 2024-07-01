#![allow(clippy::module_name_repetitions)]

use crate::app_model::mailbox::messages::MessagesState;
use crate::app_model::mailbox::model::StateHandler;
use crate::app_model::mailbox::{ConversationMessage, Item, LiveQueryBuilder, Message, ITEM_LIMIT};
use crate::app_model::BackgroundSender;
use crate::messages::Messages;
use crate::widgets::{AsTable, CenteredThrobber, ScrollableTable, ScrollableTableState};
use anyhow::anyhow;
use crossterm::event::{Event, KeyCode};
use proton_core_common::db::proton_sqlite3::Observed;
use proton_core_common::db::DBResult;
use proton_mail_common::db::{LocalConversation, LocalConversationId, LocalLabelId};
use proton_mail_common::exports::tracing;
use proton_mail_common::{MailContext, Mailbox, MailboxResult};
use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui::Frame;
use throbber_widgets_tui::ThrobberState;

/// Displays the list of conversations in the current mailbox. If a conversation is opened it
/// will display the list of messages for said conversation.
pub struct ConversationsState {
    _query: Observed,
    conversations: Vec<LocalConversation>,
    table_state: ScrollableTableState,
    messages: MessagesStatus,
}

impl ConversationsState {
    pub fn new(mbox: &Mailbox, background_sender: BackgroundSender) -> MailboxResult<Self> {
        let conversations = mbox.conversations(ITEM_LIMIT)?;
        Ok(Self {
            _query: mbox.new_conversation_query(
                LiveQueryBuilder::new(conversations_refreshed_converter, background_sender),
                ITEM_LIMIT,
            )?,
            table_state: ScrollableTableState::new(Some(0)),
            messages: MessagesStatus::None,
            conversations,
        })
    }

    fn open_conversation(
        &mut self,
        ctx: &MailContext,
        mbox: &Mailbox,
        sender: &BackgroundSender,
        id: LocalConversationId,
    ) {
        let mbox = mbox.clone();
        let sender = sender.clone();
        ctx.async_runtime().spawn(async move {
            let result = MessagesState::from_conversation(&mbox, id, sender.clone())
                .await
                .map_err(|e| {
                    let e = anyhow!("Failed to open conversation {id}: {e}");
                    tracing::error!("{e}");
                    e
                })
                .map(Box::new);
            sender.send(ConversationMessage::OpenConversationResult(result).into());
        });

        self.messages = MessagesStatus::Loading(ThrobberState::default());
    }

    fn open_conversation_result(
        &mut self,
        result: anyhow::Result<Box<MessagesState>>,
    ) -> Option<Messages> {
        match result {
            Ok(state) => {
                self.messages = MessagesStatus::Ready(state);
                None
            }
            Err(e) => {
                self.messages = MessagesStatus::None;
                Some(e.into())
            }
        }
    }

    fn close_conversation(&mut self) {
        self.messages = MessagesStatus::None;
    }

    fn conversations_refreshed(&mut self, conversations: Vec<LocalConversation>) {
        self.conversations = conversations;
    }

    pub fn draw_status_bar(&mut self, frame: &mut Frame, area: Rect) {
        if let MessagesStatus::Ready(_) = &self.messages {
            frame.render_widget(Text::from(" > Conversation Messages"), area);
        }
    }

    fn selected_conversation(&self) -> Option<LocalConversationId> {
        let index = self.table_state.selected()?;
        self.conversations.get(index).map(|c| c.id)
    }
}

impl StateHandler for ConversationsState {
    fn handle_event(&mut self, event: Event) -> Option<Messages> {
        let Event::Key(key) = &event else {
            return None;
        };
        match &mut self.messages {
            MessagesStatus::None => {}
            MessagesStatus::Loading(_) => return None,
            MessagesStatus::Ready(message_state) => {
                let is_esc = key.code == KeyCode::Esc;
                let msg = message_state.handle_event(event);
                return if msg.is_none() && is_esc {
                    Some(ConversationMessage::CloseConversation.into())
                } else {
                    msg
                };
            }
        }

        match key.code {
            KeyCode::Up => {
                self.table_state.prev();
                None
            }
            KeyCode::Down => {
                self.table_state.next();
                None
            }
            KeyCode::Char('s') => Some(Message::OpenLabelSelectPopup.into()),
            KeyCode::Char('u') => self
                .selected_conversation()
                .map(|id| ConversationMessage::MarkConversationUnread(id).into()),
            KeyCode::Char('r') => self
                .selected_conversation()
                .map(|id| ConversationMessage::MarkConversationRead(id).into()),
            KeyCode::Char('d') => self
                .selected_conversation()
                .map(|id| ConversationMessage::DeleteConversation(id).into()),
            KeyCode::Char('m') => self
                .selected_conversation()
                .map(|id| Message::OpenMoveItemPopup(Item::Conversation(id)).into()),
            KeyCode::Char('l') => self
                .selected_conversation()
                .map(|id| Message::OpenLabelItemPopup(Item::Conversation(id)).into()),
            KeyCode::Char('L') => self
                .selected_conversation()
                .map(|id| Message::OpenUnlabelItemPopup(Item::Conversation(id)).into()),
            KeyCode::Enter => self
                .selected_conversation()
                .map(|id| ConversationMessage::OpenConversation(id).into()),
            _ => None,
        }
    }

    fn update(
        &mut self,
        ctx: &MailContext,
        message: Message,
        mbox: &Mailbox,
        sender: &BackgroundSender,
    ) -> Option<Messages> {
        match &mut self.messages {
            MessagesStatus::None => {
                let Message::ConversationState(message) = message else {
                    return None;
                };

                match message {
                    ConversationMessage::MarkConversationRead(id) => {
                        mark_conversation_read(mbox, id)
                    }
                    ConversationMessage::MarkConversationUnread(id) => {
                        mark_conversation_unread(mbox, id)
                    }
                    ConversationMessage::DeleteConversation(id) => delete_conversation(mbox, id),
                    ConversationMessage::MoveConversation(id, label_id) => {
                        Some(move_conversation(mbox, id, label_id))
                    }
                    ConversationMessage::LabelConversation(id, label_id) => {
                        Some(label_conversation(mbox, id, label_id))
                    }
                    ConversationMessage::UnlabelConversation(id, label_id) => {
                        Some(unlabel_conversation(mbox, id, label_id))
                    }
                    ConversationMessage::OpenConversation(id) => {
                        self.open_conversation(ctx, mbox, sender, id);
                        None
                    }
                    ConversationMessage::Refreshed(conversations) => {
                        self.conversations_refreshed(conversations);
                        None
                    }
                    _ => None,
                }
            }

            MessagesStatus::Loading(_) => {
                if let Message::ConversationState(ConversationMessage::OpenConversationResult(r)) =
                    message
                {
                    self.open_conversation_result(r);
                }

                None
            }
            MessagesStatus::Ready(state) => {
                if let Message::ConversationState(ConversationMessage::CloseConversation) = &message
                {
                    self.close_conversation();
                    return None;
                }
                state.update(ctx, message, mbox, sender)
            }
        }
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        match &mut self.messages {
            MessagesStatus::None => {
                let table = self.conversations.as_table();
                let scrollable_table = ScrollableTable::new(table, self.conversations.len());

                frame.render_stateful_widget(scrollable_table, area, &mut self.table_state);
            }
            MessagesStatus::Loading(state) => {
                frame.render_stateful_widget(
                    CenteredThrobber::default_with_label("Loading Conversation Messages..."),
                    area,
                    state,
                );
            }
            MessagesStatus::Ready(state) => {
                state.view(frame, area);
            }
        }
    }
}

enum MessagesStatus {
    None,
    Loading(ThrobberState),
    Ready(Box<MessagesState>),
}

fn mark_conversation_read(mailbox: &Mailbox, id: LocalConversationId) -> Option<Messages> {
    match mailbox.mark_conversations_read(std::iter::once(id)) {
        Ok(()) => None,
        Err(e) => {
            let e = anyhow!("Failed to mark conversation as read: {e}");
            tracing::error!("{e}");
            Some(e.into())
        }
    }
}

fn mark_conversation_unread(mailbox: &Mailbox, id: LocalConversationId) -> Option<Messages> {
    match mailbox.mark_conversations_unread(std::iter::once(id)) {
        Ok(()) => None,
        Err(e) => {
            let e = anyhow!("Failed to mark conversation as read: {e}");
            tracing::error!("{e}");
            Some(e.into())
        }
    }
}

fn delete_conversation(mailbox: &Mailbox, id: LocalConversationId) -> Option<Messages> {
    match mailbox.delete_conversations(std::iter::once(id)) {
        Ok(()) => None,
        Err(e) => {
            let e = anyhow!("Failed to delete conversation: {e}");
            tracing::error!("{e}");
            Some(e.into())
        }
    }
}

fn move_conversation(
    mailbox: &Mailbox,
    conversation_id: LocalConversationId,
    label_id: LocalLabelId,
) -> Messages {
    match mailbox.move_conversations(label_id, std::iter::once(conversation_id)) {
        Ok(()) => Messages::DismissPopup,
        Err(e) => {
            let e = anyhow!("Failed to move conversation: {e}");
            tracing::error!("{e}");
            e.into()
        }
    }
}

fn label_conversation(
    mailbox: &Mailbox,
    conversation_id: LocalConversationId,
    label_id: LocalLabelId,
) -> Messages {
    match mailbox.label_conversations(label_id, std::iter::once(conversation_id)) {
        Ok(()) => Messages::DismissPopup,
        Err(e) => {
            let e = anyhow!("Failed to label conversation: {e}");
            tracing::error!("{e}");
            e.into()
        }
    }
}

fn unlabel_conversation(
    mailbox: &Mailbox,
    conversation_id: LocalConversationId,
    label_id: LocalLabelId,
) -> Messages {
    match mailbox.unlabel_conversations(label_id, std::iter::once(conversation_id)) {
        Ok(()) => Messages::DismissPopup,
        Err(e) => {
            let e = anyhow!("Failed to unlabel conversation: {e}");
            tracing::error!("{e}");
            e.into()
        }
    }
}

fn conversations_refreshed_converter(conversations: DBResult<Vec<LocalConversation>>) -> Messages {
    match conversations {
        Ok(c) => ConversationMessage::Refreshed(c).into(),
        Err(e) => {
            let e = anyhow!("Conversation list Query error: {e}");
            tracing::error!("{e}");
            e.into()
        }
    }
}
