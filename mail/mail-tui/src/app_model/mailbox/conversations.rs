use crate::app::Command;
use crate::app_model::YesNoPopup;
use crate::app_model::mailbox::messages::MessagesState;
use crate::app_model::mailbox::paginator::Paginator;
use crate::app_model::mailbox::{ConversationMessage, ITEM_LIMIT, Item, Message};
use crate::messages::Messages;
use crate::widgets::{AsTable, CenteredThrobber, ScrollableTable, ScrollableTableState};
use anyhow::anyhow;
use futures::FutureExt;
use proton_core_common::datatypes::LocalLabelId;
use proton_mail_common::datatypes::folder_banner::{AutoDeleteBanner, AutoDeleteState};
use proton_mail_common::datatypes::{ContextualConversation, LocalConversationId, ReadFilter};
use proton_mail_common::mail_scroller::{DataScrollerSource, MailScroller};
use proton_mail_common::models::{
    Conversation, ConversationScrollData, LabelWithCounters, Message as MailMessage,
};
use proton_mail_common::{MailContextResult, MailUserContext, Mailbox};
use ratatui::Frame;
use ratatui::crossterm::event::{Event, KeyCode};
use ratatui::layout::Rect;
use ratatui::prelude::*;
use std::sync::Arc;
use throbber_widgets_tui::ThrobberState;
use tracing::info;

use super::LabelAs;

/// Displays the list of conversations in the current mailbox. If a conversation is opened it
/// will display the list of messages for said conversation.
pub struct ConversationsState {
    paginator: Paginator<DataScrollerSource<ConversationScrollData>>,
    conversations: Vec<ContextualConversation>,
    table_state: ScrollableTableState,
    messages: MessagesStatus,
    opened_label: LocalLabelId,
    autodelete_banner: Option<AutoDeleteBanner>,
}

impl ConversationsState {
    pub(super) fn build(
        ctx: Arc<MailUserContext>,
        mbox: Mailbox,
        label: LabelWithCounters,
        filter: ReadFilter,
    ) -> Command<Messages> {
        let label_id = mbox.label_id();
        Command::task(async move {
            match Self::new_impl(ctx, label_id, filter).await {
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
        filter: ReadFilter,
    ) -> MailContextResult<(Self, Command<Messages>)> {
        let context = ctx.clone();
        let (paginator, command) = Paginator::new(
            || {
                async move {
                    MailScroller::conversations(context.as_weak(), label_id, filter, ITEM_LIMIT)
                        .await
                }
                .boxed()
            },
            move |result| match result {
                Ok(conversation) => ConversationMessage::Refreshed(conversation).into(),
                Err(e) => {
                    let e = anyhow!("Conversation Reload Query error: {e}");
                    tracing::error!("{e:?}");
                    e.into()
                }
            },
        )
        .await?;

        let autodelete_banner = ContextualConversation::auto_delete_banner(label_id, &ctx).await?;
        let conversations = paginator.fetch_more().await?;
        Ok((
            Self {
                paginator,
                table_state: ScrollableTableState::new(Some(0)),
                messages: MessagesStatus::None,
                conversations,
                opened_label: label_id,
                autodelete_banner,
            },
            command,
        ))
    }

    #[must_use]
    fn open_conversation(
        &mut self,
        ctx: Arc<MailUserContext>,
        mbox: &Mailbox,
        id: LocalConversationId,
    ) -> Command<Messages> {
        self.messages = MessagesStatus::Loading(ThrobberState::default());
        MessagesState::from_conversation(ctx, mbox, id)
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

impl ConversationsState {
    pub fn handle_event(
        &mut self,
        ctx: &Arc<MailUserContext>,
        mbox: &Mailbox,
        event: &Event,
    ) -> Command<Messages> {
        let Event::Key(key) = &event else {
            return Command::None;
        };
        match &mut self.messages {
            MessagesStatus::None => {}
            MessagesStatus::Loading(_) => return Command::None,
            MessagesStatus::Ready(message_state) => {
                let is_esc = key.code == KeyCode::Esc;
                let msg = message_state.handle_event(ctx, mbox, event);
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
            KeyCode::Char('h') => Command::message(ConversationMessage::HasMore.into()),
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
            KeyCode::Char('E') => {
                Command::message(ConversationMessage::DeleteAll(self.opened_label).into())
            }
            KeyCode::Enter => self
                .selected_conversation()
                .map(|id| Command::message(ConversationMessage::OpenConversation(id).into()))
                .unwrap_or_default(),
            _ => Command::None,
        }
    }

    pub fn update(
        &mut self,
        user_ctx: &Arc<MailUserContext>,
        message: Message,
        mbox: &Mailbox,
    ) -> Command<Messages> {
        match &mut self.messages {
            MessagesStatus::None => {
                let Message::ConversationState(message) = message else {
                    return Command::None;
                };

                match message {
                    ConversationMessage::MarkConversationRead(id) => {
                        mark_conversation_read(user_ctx.to_owned(), mbox, id)
                    }
                    ConversationMessage::MarkConversationUnread(id) => {
                        mark_conversation_unread(user_ctx.to_owned(), mbox, id)
                    }
                    ConversationMessage::DeleteConversation(id) => {
                        delete_conversation(user_ctx.to_owned(), mbox, id)
                    }
                    ConversationMessage::MoveConversation(id, label_id) => {
                        move_conversation(user_ctx.to_owned(), mbox, id, label_id)
                    }
                    ConversationMessage::LabelConversation(label_as) => {
                        label_conversation(user_ctx.to_owned(), *label_as)
                    }
                    ConversationMessage::OpenConversation(id) => {
                        self.open_conversation(user_ctx.to_owned(), mbox, id)
                    }
                    ConversationMessage::Refreshed(conversations) => {
                        self.conversations_refreshed(conversations);
                        Command::None
                    }
                    ConversationMessage::StarConversation(id) => {
                        star_conversation(user_ctx.to_owned(), id)
                    }
                    ConversationMessage::UnstarConversation(id) => {
                        unstar_conversation(user_ctx.to_owned(), id)
                    }
                    ConversationMessage::NextPage(conversations) => {
                        self.conversations.extend(conversations);
                        Command::None
                    }
                    ConversationMessage::HasMore => {
                        let paginator_clone = self.paginator.clone_paginator();
                        Command::task(async move {
                            let paginator = paginator_clone.lock().await;
                            let has_more = paginator.has_more().await.unwrap();
                            let total = paginator.total();
                            let seen = paginator.seen().await.unwrap();
                            Command::message(Messages::DisplayInfo(
                                Some("Has more".to_owned()),
                                format!("Loaded: {seen}/{total}, Has more: {has_more}"),
                            ))
                        })
                    }
                    ConversationMessage::DeleteAll(id) => delete_all(user_ctx.to_owned(), id),
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
                state.update(user_ctx, message, mbox)
            }
        }
    }

    pub fn view(&mut self, frame: &mut Frame, mut area: Rect) {
        if let Some(AutoDeleteBanner { state, folder }) = self.autodelete_banner {
            let text = match state {
                AutoDeleteState::AutoDeleteUpsell => format!(
                    "Upgrade to automatically remove emails that have been in {folder} for over 30 days."
                ),
                AutoDeleteState::AutoDeleteDisabled => format!(
                    "Auto-delete is off. Messages in {folder} will remain until you delete them manually."
                ),
                AutoDeleteState::AutoDeleteEnabled => {
                    format!("Messages in {folder} will be automatically deleted after 30 days.")
                }
            };
            let [para_area, rest] = Layout::default()
                .constraints([Constraint::Length(1), Constraint::Percentage(100)])
                .areas(area);

            frame.render_widget(ratatui::widgets::Paragraph::new(text), para_area);
            area = rest;
        }

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

    pub fn help_options(&self, vec: &mut Vec<(&'static str, &'static str)>) {
        info!("This was called!");
        if let MessagesStatus::Ready(message_state) = &self.messages {
            message_state.help_options(vec);
        } else {
            vec.push(("E", "Permanently delete all messages here"));
        }
    }
}

enum MessagesStatus {
    None,
    Loading(ThrobberState),
    Ready(Box<MessagesState>),
}

fn mark_conversation_read(
    ctx: Arc<MailUserContext>,
    mailbox: &Mailbox,
    id: LocalConversationId,
) -> Command<Messages> {
    let local_label_id = mailbox.label_id();
    Command::task(async move {
        match Conversation::action_mark_read(ctx.action_queue(), local_label_id, vec![id]).await {
            Ok(()) => Command::None,
            Err(e) => {
                let e = anyhow!("Failed to mark conversation as read: {e}");
                tracing::error!("{e:?}");
                Command::message(e.into())
            }
        }
    })
}

fn mark_conversation_unread(
    ctx: Arc<MailUserContext>,
    mailbox: &Mailbox,
    id: LocalConversationId,
) -> Command<Messages> {
    let current_label_id = mailbox.label_id();
    Command::task(async move {
        match Conversation::action_mark_unread(ctx.action_queue(), current_label_id, vec![id]).await
        {
            Ok(()) => Command::None,
            Err(e) => {
                let e = anyhow!("Failed to mark conversation as read: {e}");
                tracing::error!("{e:?}");
                Command::message(e.into())
            }
        }
    })
}

fn delete_conversation(
    ctx: Arc<MailUserContext>,
    mailbox: &Mailbox,
    id: LocalConversationId,
) -> Command<Messages> {
    let current_label_id = mailbox.label_id();
    Command::message(Messages::raise_popup(
        YesNoPopup::new(
            "Confirm Conversation Delete",
            "Are you sure you wish to permanently delete the currently selected conversation?",
        )
        .on_accept(Command::task(async move {
            match Conversation::action_mark_deleted(ctx.action_queue(), current_label_id, [id])
                .await
            {
                Ok(_) => Command::None,
                Err(e) => {
                    let e = anyhow!("Failed to delete conversation: {e}");
                    tracing::error!("{e:?}");
                    Command::message(e.into())
                }
            }
        })),
    ))
}

fn move_conversation(
    ctx: Arc<MailUserContext>,
    mailbox: &Mailbox,
    conversation_id: LocalConversationId,
    label_id: LocalLabelId,
) -> Command<Messages> {
    let current_label_id = mailbox.label_id();
    Command::task(async move {
        match Conversation::action_move(
            ctx.action_queue(),
            current_label_id,
            label_id,
            vec![conversation_id],
        )
        .await
        {
            Ok(_) => Command::None,
            Err(e) => {
                let e = anyhow!("Failed to move conversation: {e}");
                tracing::error!("{e:?}");
                Command::message(e.into())
            }
        }
    })
}

fn star_conversation(
    ctx: Arc<MailUserContext>,
    conversation_id: LocalConversationId,
) -> Command<Messages> {
    Command::task(async move {
        match Conversation::action_star(ctx.action_queue(), vec![conversation_id]).await {
            Ok(_) => Command::None,
            Err(e) => {
                let e = anyhow!("Failed to label conversation: {e}");
                tracing::error!("{e:?}");
                Command::message(e.into())
            }
        }
    })
}

fn unstar_conversation(
    ctx: Arc<MailUserContext>,
    conversation_id: LocalConversationId,
) -> Command<Messages> {
    Command::task(async move {
        match Conversation::action_unstar(ctx.action_queue(), vec![conversation_id]).await {
            Ok(_) => Command::None,
            Err(e) => {
                let e = anyhow!("Failed to label conversation: {e}");
                tracing::error!("{e:?}");
                Command::message(e.into())
            }
        }
    })
}

fn label_conversation(
    ctx: Arc<MailUserContext>,
    LabelAs {
        source_label_id,
        item_ids: conversation_ids,
        selected_label_ids,
        partially_selected_label_ids,
        must_archive,
    }: LabelAs<LocalConversationId>,
) -> Command<Messages> {
    Command::task(async move {
        match Conversation::action_label_as(
            ctx.action_queue(),
            source_label_id,
            conversation_ids,
            selected_label_ids,
            partially_selected_label_ids,
            must_archive,
        )
        .await
        {
            Ok(_) => Command::None,
            Err(e) => {
                let e = anyhow!("Failed to label conversation: {e}");
                tracing::error!("{e:?}");
                Command::message(e.into())
            }
        }
    })
}

fn delete_all(ctx: Arc<MailUserContext>, id: LocalLabelId) -> Command<Messages> {
    Command::message(Messages::raise_popup(
        YesNoPopup::new(
            "Confirm Delete All",
            "Are you sure you wish to permanently delete all messages of this folder?",
        )
        .on_accept(Command::task(async move {
            match MailMessage::action_delete_all_in_label(ctx.action_queue(), id).await {
                Ok(_) => Command::None,
                Err(e) => {
                    let e = anyhow!("Failed to delete all in label: {e}");
                    tracing::error!("{e:?}");
                    Command::message(e.into())
                }
            }
        })),
    ))
}
