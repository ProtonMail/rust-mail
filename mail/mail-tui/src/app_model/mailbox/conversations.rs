#![allow(clippy::module_name_repetitions)]

use crate::app::Command;
use crate::app_model::mailbox::messages::MessagesState;
use crate::app_model::mailbox::model::StateHandler;
use crate::app_model::mailbox::paginator::Paginator;
use crate::app_model::mailbox::{ConversationMessage, Item, Message, ITEM_LIMIT};
use crate::app_model::YesNoPopup;
use crate::messages::Messages;
use crate::widgets::{AsTable, CenteredThrobber, ScrollableTable, ScrollableTableState};
use anyhow::anyhow;
use futures::FutureExt;
use proton_core_common::datatypes::LocalLabelId;
use proton_mail_common::datatypes::{ContextualConversation, LocalConversationId, ReadFilter};
use proton_mail_common::mail_scroller::{MailConversationScrollerSource, MailScroller};
use proton_mail_common::models::{Conversation, Label, MailSettings};
use proton_mail_common::{MailContext, MailUserContext, Mailbox, MailboxResult};
use ratatui::crossterm::event::{Event, KeyCode};
use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui::Frame;
use std::sync::Arc;
use throbber_widgets_tui::ThrobberState;

use super::LabelAs;

/// Displays the list of conversations in the current mailbox. If a conversation is opened it
/// will display the list of messages for said conversation.
pub struct ConversationsState {
    paginator: Paginator<MailConversationScrollerSource>,
    conversations: Vec<ContextualConversation>,
    table_state: ScrollableTableState,
    messages: MessagesStatus,
}

impl ConversationsState {
    pub(super) fn build(mbox: Mailbox, label: Label) -> Command<Messages> {
        let ctx = mbox.user_context();
        let label_id = mbox.label_id();
        Command::task(async move {
            match Self::new_impl(ctx, label_id).await {
                Ok((state, background_command)) => Command::batch([
                    Command::message(Message::OpenConversationView(mbox, label, state).into()),
                    background_command,
                ]),
                Err(e) => Command::message(e.into()),
            }
        })
    }
    async fn new_impl(
        ctx: Arc<MailUserContext>,
        label_id: LocalLabelId,
    ) -> MailboxResult<(Self, Command<Messages>)> {
        let context = ctx.clone();
        let (paginator, command) = Paginator::new(
            || {
                async move {
                    let source =
                        MailConversationScrollerSource::new(label_id, ReadFilter::All, ITEM_LIMIT);

                    MailScroller::new(context, source).await
                }
                .boxed()
            },
            move |result| match result {
                Ok(conversation) => ConversationMessage::Refreshed(conversation).into(),
                Err(e) => {
                    let e = anyhow!("Conversation Reload Query error: {e}");
                    tracing::error!("{e}");
                    e.into()
                }
            },
        )
        .await?;

        let conversations = paginator.all_items().await?;
        Ok((
            Self {
                paginator,
                table_state: ScrollableTableState::new(Some(0)),
                messages: MessagesStatus::None,
                conversations,
            },
            command,
        ))
    }

    #[must_use]
    fn open_conversation(&mut self, mbox: &Mailbox, id: LocalConversationId) -> Command<Messages> {
        self.messages = MessagesStatus::Loading(ThrobberState::default());
        MessagesState::from_conversation(mbox, id)
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

    fn selected_conversation(&self) -> Option<LocalConversationId> {
        let index = self.table_state.selected()?;
        self.conversations.get(index).map(|c| c.local_id)
    }
}

impl StateHandler for ConversationsState {
    fn handle_event(&mut self, mbox: &Mailbox, event: Event) -> Command<Messages> {
        let Event::Key(key) = &event else {
            return Command::None;
        };
        match &mut self.messages {
            MessagesStatus::None => {}
            MessagesStatus::Loading(_) => return Command::None,
            MessagesStatus::Ready(message_state) => {
                let is_esc = key.code == KeyCode::Esc;
                let msg = message_state.handle_event(mbox, event);
                return if msg.is_none() && is_esc {
                    Command::message(ConversationMessage::CloseConversation.into())
                } else {
                    msg
                };
            }
        }

        match key.code {
            KeyCode::Char('k') | KeyCode::Up => {
                self.table_state.prev();
                Command::None
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.table_state.next();
                if self.table_state.selected().unwrap_or_default()
                    == self.conversations.len().saturating_sub(1)
                {
                    return self.paginator.next_page_command(move |v| {
                        Command::message(ConversationMessage::NextPage(v).into())
                    });
                }
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
            KeyCode::Char('f') => self
                .selected_conversation()
                .map(|id| Command::message(ConversationMessage::StarConversation(id).into()))
                .unwrap_or_default(),
            KeyCode::Char('F') => self
                .selected_conversation()
                .map(|id| Command::message(ConversationMessage::UnstarConversation(id).into()))
                .unwrap_or_default(),
            KeyCode::Enter => self
                .selected_conversation()
                .map(|id| Command::message(ConversationMessage::OpenConversation(id).into()))
                .unwrap_or_default(),
            _ => Command::None,
        }
    }

    fn update(
        &mut self,
        ctx: &MailContext,
        message: Message,
        mbox: &Mailbox,
        mail_settings: &Arc<MailSettings>,
    ) -> Command<Messages> {
        match &mut self.messages {
            MessagesStatus::None => {
                let Message::ConversationState(message) = message else {
                    return Command::None;
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
                        move_conversation(mbox, id, label_id)
                    }
                    ConversationMessage::LabelConversation(label_as) => {
                        label_conversation(mbox, *label_as)
                    }
                    ConversationMessage::OpenConversation(id) => self.open_conversation(mbox, id),
                    ConversationMessage::Refreshed(conversations) => {
                        self.conversations_refreshed(conversations);
                        Command::None
                    }
                    ConversationMessage::StarConversation(id) => star_conversation(mbox, id),
                    ConversationMessage::UnstarConversation(id) => unstar_conversation(mbox, id),
                    ConversationMessage::NextPage(conversations) => {
                        self.conversations.extend(conversations);
                        Command::None
                    }
                    _ => Command::None,
                }
            }

            MessagesStatus::Loading(_) => match message {
                Message::ConversationState(ConversationMessage::OpenConversationSuccess(state)) => {
                    self.messages = MessagesStatus::Ready(state);
                    Command::None
                }
                Message::ConversationState(ConversationMessage::OpenConversationFailed(e)) => {
                    self.messages = MessagesStatus::None;
                    Command::message(Messages::DisplayError(None, e))
                }
                _ => Command::None,
            },
            MessagesStatus::Ready(state) => {
                if let Message::ConversationState(ConversationMessage::CloseConversation) = &message
                {
                    self.close_conversation();
                    return Command::None;
                }
                state.update(ctx, message, mbox, mail_settings)
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

fn mark_conversation_read(mailbox: &Mailbox, id: LocalConversationId) -> Command<Messages> {
    let ctx = mailbox.user_context();
    let local_label_id = mailbox.label_id();
    Command::task(async move {
        match ctx
            .with_queue(|queue| Conversation::action_mark_read(queue, local_label_id, vec![id]))
            .await
        {
            Ok(_) => Command::None,
            Err(e) => {
                let e = anyhow!("Failed to mark conversation as read: {e}");
                tracing::error!("{e}");
                Command::message(e.into())
            }
        }
    })
}

fn mark_conversation_unread(mailbox: &Mailbox, id: LocalConversationId) -> Command<Messages> {
    let ctx = mailbox.user_context();
    let current_label_id = mailbox.label_id();
    Command::task(async move {
        match ctx
            .with_queue(|queue| Conversation::action_mark_unread(queue, current_label_id, vec![id]))
            .await
        {
            Ok(_) => Command::None,
            Err(e) => {
                let e = anyhow!("Failed to mark conversation as read: {e}");
                tracing::error!("{e}");
                Command::message(e.into())
            }
        }
    })
}

fn delete_conversation(mailbox: &Mailbox, id: LocalConversationId) -> Command<Messages> {
    let ctx = mailbox.user_context();
    let current_label_id = mailbox.label_id();
    Command::message(Messages::raise_popup(
        YesNoPopup::new(
            "Confirm Conversation Delete",
            "Are you sure you wish to permanently delete the currently selected conversation?",
        )
        .on_accept(Command::task(async move {
            match ctx
                .with_queue(|queue| {
                    Conversation::action_mark_deleted(queue, current_label_id, std::iter::once(id))
                })
                .await
            {
                Ok(_) => Command::None,
                Err(e) => {
                    let e = anyhow!("Failed to delete conversation: {e}");
                    tracing::error!("{e}");
                    Command::message(e.into())
                }
            }
        })),
    ))
}

fn move_conversation(
    mailbox: &Mailbox,
    conversation_id: LocalConversationId,
    label_id: LocalLabelId,
) -> Command<Messages> {
    let ctx = mailbox.user_context();
    let current_label_id = mailbox.label_id();
    Command::task(async move {
        match ctx
            .with_queue(|queue| {
                Conversation::action_move(queue, current_label_id, label_id, vec![conversation_id])
            })
            .await
        {
            Ok(_) => Command::None,
            Err(e) => {
                let e = anyhow!("Failed to move conversation: {e}");
                tracing::error!("{e}");
                Command::message(e.into())
            }
        }
    })
}

fn star_conversation(mailbox: &Mailbox, conversation_id: LocalConversationId) -> Command<Messages> {
    let ctx = mailbox.user_context();
    Command::task(async move {
        match ctx
            .with_queue(|queue| Conversation::action_star(queue, vec![conversation_id]))
            .await
        {
            Ok(_) => Command::None,
            Err(e) => {
                let e = anyhow!("Failed to label conversation: {e}");
                tracing::error!("{e}");
                Command::message(e.into())
            }
        }
    })
}

fn unstar_conversation(
    mailbox: &Mailbox,
    conversation_id: LocalConversationId,
) -> Command<Messages> {
    let ctx = mailbox.user_context();
    Command::task(async move {
        match ctx
            .with_queue(|queue| Conversation::action_unstar(queue, vec![conversation_id]))
            .await
        {
            Ok(_) => Command::None,
            Err(e) => {
                let e = anyhow!("Failed to label conversation: {e}");
                tracing::error!("{e}");
                Command::message(e.into())
            }
        }
    })
}

fn label_conversation(
    mailbox: &Mailbox,
    LabelAs {
        source_label_id,
        item_ids: conversation_ids,
        selected_label_ids,
        partially_selected_label_ids,
        must_archive,
    }: LabelAs<LocalConversationId>,
) -> Command<Messages> {
    let ctx = mailbox.user_context();
    Command::task(async move {
        match ctx
            .with_queue(|queue| {
                Conversation::action_label_as(
                    queue,
                    source_label_id,
                    conversation_ids,
                    selected_label_ids,
                    partially_selected_label_ids,
                    must_archive,
                )
            })
            .await
        {
            Ok(_) => Command::None,
            Err(e) => {
                let e = anyhow!("Failed to label conversation: {e}");
                tracing::error!("{e}");
                Command::message(e.into())
            }
        }
    })
}
