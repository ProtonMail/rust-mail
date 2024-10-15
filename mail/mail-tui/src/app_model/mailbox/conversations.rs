#![allow(clippy::module_name_repetitions)]

use crate::app::Command;
use crate::app_model::mailbox::messages::MessagesState;
use crate::app_model::mailbox::model::StateHandler;
use crate::app_model::mailbox::{BackgroundSender, ConversationMessage, Item, Message};
use crate::app_model::watcher::WatchHandle;
use crate::messages::Messages;
use crate::widgets::{AsTable, CenteredThrobber, ScrollableTable, ScrollableTableState};
use anyhow::anyhow;
use crossterm::event::{Event, KeyCode};
use futures::FutureExt;
use proton_core_common::datatypes::LocalId;
use proton_mail_common::datatypes::ContextualConversation;
use proton_mail_common::models::{Conversation, MailSettings};
use proton_mail_common::{MailContext, Mailbox, MailboxResult};
use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui::Frame;
use std::sync::Arc;
use throbber_widgets_tui::ThrobberState;

/// Displays the list of conversations in the current mailbox. If a conversation is opened it
/// will display the list of messages for said conversation.
pub struct ConversationsState {
    _query: WatchHandle,
    conversations: Vec<ContextualConversation>,
    table_state: ScrollableTableState,
    messages: MessagesStatus,
}

impl ConversationsState {
    pub async fn new(mbox: &Mailbox, sender: BackgroundSender) -> MailboxResult<Self> {
        let (conversations, receiver) = ContextualConversation::watch_in_label(
            mbox.label_id(),
            mbox.user_context().user_stash(),
        )
        .await?;
        let ctx = mbox.user_context();
        let label_id = mbox.label_id();
        let watcher = WatchHandle::new_dampened(
            receiver,
            move || {
                let ctx = Arc::clone(&ctx);
                async move {
                    Some(
                        match ContextualConversation::in_label(label_id, ctx.user_stash()).await {
                            Ok(c) => ConversationMessage::Refreshed(c).into(),
                            Err(e) => {
                                let e = anyhow!("Conversation list Query error: {e}");
                                tracing::error!("{e}");
                                e.into()
                            }
                        },
                    )
                }
                .boxed()
            },
            sender,
        );
        Ok(Self {
            _query: watcher,
            table_state: ScrollableTableState::new(Some(0)),
            messages: MessagesStatus::None,
            conversations,
        })
    }

    #[must_use]
    fn open_conversation(
        &mut self,
        mbox: &Mailbox,
        id: LocalId,
        sender: BackgroundSender,
    ) -> Command<Messages> {
        self.messages = MessagesStatus::Loading(ThrobberState::default());
        let mbox = mbox.clone();
        Command::task(async move {
            let result = MessagesState::from_conversation(&mbox, id, sender)
                .await
                .map_err(|e| {
                    let e = anyhow!("Failed to open conversation {id}: {e}");
                    tracing::error!("{e}");
                    e
                })
                .map(Box::new);
            Command::message(ConversationMessage::OpenConversationResult(result).into())
        })
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

    fn conversations_refreshed(&mut self, conversations: Vec<ContextualConversation>) {
        self.conversations = conversations;
    }

    pub fn draw_status_bar(&mut self, frame: &mut Frame, area: Rect) {
        if let MessagesStatus::Ready(_) = &self.messages {
            frame.render_widget(Text::from(" > Conversation Messages"), area);
        }
    }

    fn selected_conversation(&self) -> Option<LocalId> {
        let index = self.table_state.selected()?;
        self.conversations.get(index).map(|c| c.local_id)
    }
}

impl StateHandler for ConversationsState {
    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        let Event::Key(key) = &event else {
            return Command::None;
        };
        match &mut self.messages {
            MessagesStatus::None => {}
            MessagesStatus::Loading(_) => return Command::None,
            MessagesStatus::Ready(message_state) => {
                let is_esc = key.code == KeyCode::Esc;
                let msg = message_state.handle_event(event);
                return if msg.is_none() && is_esc {
                    Command::message(ConversationMessage::CloseConversation.into())
                } else {
                    msg
                };
            }
        }

        match key.code {
            KeyCode::Up => {
                self.table_state.prev();
                Command::None
            }
            KeyCode::Down => {
                self.table_state.next();
                Command::None
            }
            KeyCode::Char('s') => Command::message(Message::OpenLabelSelectPopup.into()),
            KeyCode::Char('u') => self
                .selected_conversation()
                .map(|id| Command::message(ConversationMessage::MarkConversationUnread(id).into()))
                .unwrap_or_default(),
            KeyCode::Char('r') => self
                .selected_conversation()
                .map(|id| Command::message(ConversationMessage::MarkConversationRead(id).into()))
                .unwrap_or_default(),
            KeyCode::Char('d') => self
                .selected_conversation()
                .map(|id| Command::message(ConversationMessage::DeleteConversation(id).into()))
                .unwrap_or_default(),
            KeyCode::Char('m') => self
                .selected_conversation()
                .map(|id| {
                    Command::message(Message::OpenMoveItemPopup(Item::Conversation(id)).into())
                })
                .unwrap_or_default(),
            KeyCode::Char('l') => self
                .selected_conversation()
                .map(|id| {
                    Command::message(Message::OpenLabelItemPopup(Item::Conversation(id)).into())
                })
                .unwrap_or_default(),
            KeyCode::Char('L') => self
                .selected_conversation()
                .map(|id| {
                    Command::message(Message::OpenUnlabelItemPopup(Item::Conversation(id)).into())
                })
                .unwrap_or_default(),
            KeyCode::Enter => self
                .selected_conversation()
                .map(|id| Command::message(ConversationMessage::OpenConversation(id).into()))
                .unwrap_or_default(),
            _ => Command::None,
        }
    }

    async fn update(
        &mut self,
        ctx: &MailContext,
        message: Message,
        mbox: &Mailbox,
        mail_settings: &Arc<MailSettings>,
        sender: &BackgroundSender,
    ) -> Command<Messages> {
        match &mut self.messages {
            MessagesStatus::None => {
                let Message::ConversationState(message) = message else {
                    return Command::None;
                };

                match message {
                    ConversationMessage::MarkConversationRead(id) => {
                        mark_conversation_read(mbox, id).await
                    }
                    ConversationMessage::MarkConversationUnread(id) => {
                        mark_conversation_unread(mbox, id).await
                    }
                    ConversationMessage::DeleteConversation(id) => {
                        delete_conversation(mbox, id).await
                    }
                    ConversationMessage::MoveConversation(id, label_id) => {
                        move_conversation(mbox, id, label_id).await
                    }
                    ConversationMessage::LabelConversation(id, label_id) => {
                        label_conversation(mbox, id, label_id).await
                    }
                    ConversationMessage::UnlabelConversation(id, label_id) => {
                        unlabel_conversation(mbox, id, label_id).await
                    }
                    ConversationMessage::OpenConversation(id) => {
                        self.open_conversation(mbox, id, sender.clone())
                    }
                    ConversationMessage::Refreshed(conversations) => {
                        self.conversations_refreshed(conversations);
                        Command::None
                    }
                    _ => Command::None,
                }
            }

            MessagesStatus::Loading(_) => {
                if let Message::ConversationState(ConversationMessage::OpenConversationResult(r)) =
                    message
                {
                    self.open_conversation_result(r);
                }

                Command::None
            }
            MessagesStatus::Ready(state) => {
                if let Message::ConversationState(ConversationMessage::CloseConversation) = &message
                {
                    self.close_conversation();
                    return Command::None;
                }
                state
                    .update(ctx, message, mbox, mail_settings, sender)
                    .await
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

#[allow(unused_variables)]
async fn mark_conversation_read(mailbox: &Mailbox, id: LocalId) -> Command<Messages> {
    match Conversation::action_mark_read(
        mailbox.user_context().session(),
        mailbox.user_context().queue(),
        mailbox.label_id(),
        vec![id],
    )
    .await
    {
        Ok(_) => Command::None,
        Err(e) => {
            let e = anyhow!("Failed to mark conversation as read: {e}");
            tracing::error!("{e}");
            Command::message(e.into())
        }
    }
}

#[allow(unused_variables)]
async fn mark_conversation_unread(mailbox: &Mailbox, id: LocalId) -> Command<Messages> {
    match Conversation::action_mark_unread(
        mailbox.user_context().session(),
        mailbox.user_context().queue(),
        mailbox.label_id(),
        vec![id],
    )
    .await
    {
        Ok(_) => Command::None,
        Err(e) => {
            let e = anyhow!("Failed to mark conversation as read: {e}");
            tracing::error!("{e}");
            Command::message(e.into())
        }
    }
}

#[allow(unused_variables)]
async fn delete_conversation(mailbox: &Mailbox, id: LocalId) -> Command<Messages> {
    match Conversation::action_mark_deleted(
        mailbox.user_context().session(),
        mailbox.user_context().queue(),
        mailbox.label_id(),
        std::iter::once(id),
    )
    .await
    {
        Ok(_) => Command::None,
        Err(e) => {
            let e = anyhow!("Failed to delete conversation: {e}");
            tracing::error!("{e}");
            Command::message(e.into())
        }
    }
}

#[allow(unused_variables)]
async fn move_conversation(
    mailbox: &Mailbox,
    conversation_id: LocalId,
    label_id: LocalId,
) -> Command<Messages> {
    match Conversation::action_move(
        mailbox.user_context().session(),
        mailbox.user_context().queue(),
        mailbox.label_id(),
        label_id,
        vec![conversation_id],
    )
    .await
    {
        Ok(_) => Command::None,
        Err(e) => {
            let e = anyhow!("Failed to move conversation: {e}");
            tracing::error!("{e}");
            Command::message(e.into())
        }
    }
}

#[allow(unused_variables)]
async fn label_conversation(
    mailbox: &Mailbox,
    conversation_id: LocalId,
    label_id: LocalId,
) -> Command<Messages> {
    match Conversation::action_apply_label(
        mailbox.user_context().session(),
        mailbox.user_context().queue(),
        mailbox.label_id(),
        vec![conversation_id],
    )
    .await
    {
        Ok(_) => Command::None,
        Err(e) => {
            let e = anyhow!("Failed to label conversation: {e}");
            tracing::error!("{e}");
            Command::message(e.into())
        }
    }
}

#[allow(unused_variables)]
async fn unlabel_conversation(
    mailbox: &Mailbox,
    conversation_id: LocalId,
    label_id: LocalId,
) -> Command<Messages> {
    match Conversation::action_remove_label(
        mailbox.user_context().session(),
        mailbox.user_context().queue(),
        mailbox.label_id(),
        vec![conversation_id],
    )
    .await
    {
        Ok(_) => Command::None,
        Err(e) => {
            let e = anyhow!("Failed to unlabel conversation: {e}");
            tracing::error!("{e}");
            Command::message(e.into())
        }
    }
}
