use super::LabelAs;
use crate::app::Command;
use crate::app_model::YesNoPopup;
use crate::app_model::mailbox::messages::MessagesState;
use crate::app_model::mailbox::paginator::Paginator;
use crate::app_model::mailbox::{ConversationMessage, ITEM_LIMIT, Items, Message};
use crate::messages::Messages;
use crate::widgets::utils::ScrollableState;
use crate::widgets::{AsIntoTable, CenteredThrobber, ScrollableTable, ScrollableTableState};
use anyhow::{Context, anyhow};
use crossterm::event::KeyModifiers;
use proton_core_common::datatypes::LocalLabelId;
use proton_mail_common::datatypes::folder_banner::{AutoDeleteBanner, AutoDeleteState};
use proton_mail_common::datatypes::{
    ContextualConversation, IncludeSwitch, LocalConversationId, ReadFilter,
};
use proton_mail_common::mail_scroller::{MailScroller, ScrollerUpdate};
use proton_mail_common::models::{Conversation, LabelWithCounters, Message as MailMessage};
use proton_mail_common::{MailContextResult, MailUserContext, Mailbox};
use ratatui::Frame;
use ratatui::crossterm::event::{Event, KeyCode};
use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;
use std::sync::Arc;
use throbber_widgets_tui::ThrobberState;

/// Displays the list of conversations in the current mailbox. If a conversation is opened it
/// will display the list of messages for said conversation.
pub struct ConversationsState {
    paginator: Paginator,
    include: IncludeSwitch,
    conversations: Vec<ContextualConversation>,
    table_state: ScrollableTableState,
    messages: MessagesStatus,
    opened_label: LocalLabelId,
    autodelete_banner: Option<AutoDeleteBanner>,
    fetching: bool,
}

impl ConversationsState {
    pub(super) fn build(
        ctx: Arc<MailUserContext>,
        mbox: Mailbox,
        label: LabelWithCounters,
        unread: ReadFilter,
    ) -> Command<Messages> {
        let label_id = mbox.label_id();

        Command::task(async move {
            match Self::new_impl(ctx, label_id, unread).await {
                Ok((state, background_command)) => Command::batch([
                    Command::message(Message::OpenConversationView(mbox, label, state)),
                    background_command,
                ]),
                Err(e) => Command::message(e),
            }
        })
    }

    pub fn paginator(&self) -> &Paginator {
        &self.paginator
    }

    async fn new_impl(
        ctx: Arc<MailUserContext>,
        label_id: LocalLabelId,
        unread: ReadFilter,
    ) -> MailContextResult<(Self, Command<Messages>)> {
        let (scroller, handle) = MailScroller::conversations(
            ctx.as_weak(),
            label_id,
            unread,
            IncludeSwitch::default(),
            ITEM_LIMIT,
        )
        .await?;

        let (paginator, command) = Paginator::new(scroller, handle, |update| match update {
            ScrollerUpdate::Append { src: _, items } => ConversationMessage::NextPage(items).into(),
            ScrollerUpdate::ReplaceFrom { src: _, idx, items } => {
                ConversationMessage::ReplaceFrom(idx, items).into()
            }
            ScrollerUpdate::ReplaceBefore { src: _, idx, items } => {
                ConversationMessage::ReplaceBefore(idx, items).into()
            }
            ScrollerUpdate::ReplaceRange {
                src: _,
                from,
                to,
                items,
            } => ConversationMessage::ReplaceRange(from, to, items).into(),
            ScrollerUpdate::Error { src, error } => {
                let e = anyhow!("Conversation Reload Query src: {src:?}, error: {error}");
                tracing::error!("{e:?}");
                e.into()
            }
            ScrollerUpdate::None(_) => ConversationMessage::NextPage(vec![]).into(),
        });

        let autodelete_banner = ContextualConversation::auto_delete_banner(label_id, &ctx).await?;

        paginator.next_page_command();

        Ok((
            Self {
                paginator,
                include: IncludeSwitch::default(),
                table_state: ScrollableTableState::new(Some(0)),
                messages: MessagesStatus::None,
                conversations: vec![],
                opened_label: label_id,
                autodelete_banner,
                fetching: false,
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

    pub fn draw_status_bar(&mut self, frame: &mut Frame, area: Rect) {
        let len = self.table_state.marked.len();
        if len > 0 {
            frame.render_widget(Text::from(format!(" {len} items selected |")), area);
        }

        if let MessagesStatus::Ready(_) = &self.messages {
            frame.render_widget(Text::from(" > Conversation Messages"), area);
        }
    }

    fn selected_id_and(
        &self,
        and: impl Fn(LocalConversationId) -> Command<Messages>,
    ) -> Command<Messages> {
        let Some(idx) = self.table_state.selected() else {
            return Command::none();
        };

        and(self.conversations[idx].local_id)
    }

    /// Gets the selected conversations and unselects them.
    fn convs(&mut self) -> Vec<LocalConversationId> {
        self.table_state
            .take_selected_items(&|idx| self.conversations[idx].local_id)
    }

    fn try_select_non_empty_list(&mut self) -> Command<Messages> {
        if self.table_state.selected().is_none() {
            self.table_state.select(0);
        }

        Command::None
    }

    fn on_next_page(&mut self, conversations: Vec<ContextualConversation>) -> Command<Messages> {
        self.fetching = false;
        self.conversations.extend(conversations);
        self.try_select_non_empty_list()
    }

    fn on_replace_from(
        &mut self,
        idx: usize,
        conversations: Vec<ContextualConversation>,
    ) -> Command<Messages> {
        self.conversations.splice(idx.., conversations);
        self.try_select_non_empty_list()
    }

    fn on_replace_before(
        &mut self,
        idx: usize,
        conversations: Vec<ContextualConversation>,
    ) -> Command<Messages> {
        self.conversations.splice(..idx, conversations);
        self.try_select_non_empty_list()
    }

    fn on_replace_range(
        &mut self,
        from: usize,
        to: usize,
        conversations: Vec<ContextualConversation>,
    ) -> Command<Messages> {
        self.conversations.splice(from..to, conversations);
        self.try_select_non_empty_list()
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
                    Command::message(ConversationMessage::Close)
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
                    >= self.conversations.len().saturating_sub(1)
                    && !self.fetching
                {
                    self.fetching = true;
                    return self.paginator.next_page_command();
                }

                Command::None
            }

            KeyCode::Char(' ') => {
                self.table_state.toggle();
                Command::None
            }

            KeyCode::Char('a') => {
                self.table_state.mark_many(0..self.conversations.len());
                Command::None
            }

            KeyCode::Char('A') => {
                self.table_state.unmark_many(0..self.conversations.len());
                Command::None
            }

            KeyCode::Char('s') => Message::OpenLabelSelectPopup.into(),

            KeyCode::Char('m') => {
                Message::OpenMoveItemsPopup(Items::Conversation(self.convs())).into()
            }

            KeyCode::Char('l') => {
                Message::OpenLabelItemPopup(Items::Conversation(self.convs())).into()
            }

            KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                ConversationMessage::HasMore.into()
            }

            KeyCode::Char('u') => ConversationMessage::MarkUnread(self.convs()).into(),
            KeyCode::Char('r') => ConversationMessage::MarkRead(self.convs()).into(),
            KeyCode::Char('d') => ConversationMessage::DeletePermanently(self.convs()).into(),
            KeyCode::Char('f') => ConversationMessage::Star(self.convs()).into(),
            KeyCode::Char('F') => ConversationMessage::Unstar(self.convs()).into(),
            KeyCode::Char('X') => ConversationMessage::DeleteAll(self.opened_label).into(),

            KeyCode::Char(ch @ ('E' | 'I')) => {
                self.include = match ch {
                    'E' => IncludeSwitch::Default,
                    'I' => IncludeSwitch::WithSpamAndTrash,
                    _ => unreachable!(),
                };

                _ = self.paginator.change_include(self.include);

                Command::None
            }

            KeyCode::Char('z') => {
                Message::OpenSnoozePopup(Items::Conversation(self.convs())).into()
            }

            KeyCode::Enter => self.selected_id_and(|id| ConversationMessage::Open(id).into()),

            _ => Command::None,
        }
    }

    #[allow(clippy::too_many_lines)]
    pub fn update(
        &mut self,
        user_ctx: &Arc<MailUserContext>,
        message: Message,
        mbox: &Mailbox,
    ) -> Command<Messages> {
        // Apply the scroller updates always, as changes can still happen
        // when the message view is open.
        let message = match message {
            Message::ConversationState(ConversationMessage::NextPage(conversations)) => {
                return self.on_next_page(conversations);
            }
            Message::ConversationState(ConversationMessage::ReplaceFrom(idx, conversations)) => {
                return self.on_replace_from(idx, conversations);
            }
            Message::ConversationState(ConversationMessage::ReplaceBefore(idx, conversations)) => {
                return self.on_replace_before(idx, conversations);
            }
            m => m,
        };

        match &mut self.messages {
            MessagesStatus::None => {
                let Message::ConversationState(message) = message else {
                    return Command::None;
                };

                match message {
                    ConversationMessage::MarkRead(id) => {
                        mark_conversation_read(user_ctx.to_owned(), mbox, id)
                    }
                    ConversationMessage::MarkUnread(id) => {
                        mark_conversation_unread(user_ctx.to_owned(), mbox, id)
                    }
                    ConversationMessage::DeletePermanently(id) => {
                        delete_conversation(user_ctx.to_owned(), mbox, id)
                    }
                    ConversationMessage::MoveTo(id, label_id) => {
                        move_conversation(user_ctx.to_owned(), id, label_id)
                    }
                    ConversationMessage::LabelAs(label_as) => {
                        label_conversation(user_ctx.to_owned(), *label_as)
                    }
                    ConversationMessage::Open(id) => {
                        self.open_conversation(user_ctx.to_owned(), mbox, id)
                    }
                    ConversationMessage::Star(id) => star_conversation(user_ctx.to_owned(), id),
                    ConversationMessage::Unstar(id) => unstar_conversation(user_ctx.to_owned(), id),
                    ConversationMessage::Snooze(id, timestamp, local_label_id) => {
                        let user_ctx = user_ctx.clone();
                        Command::command_from_future(async move {
                            Conversation::action_snooze(
                                user_ctx.action_queue(),
                                local_label_id,
                                id,
                                timestamp,
                            )
                            .await
                            .context("Failed to snooze conversation")
                            .map(|_| Command::None)
                        })
                    }
                    ConversationMessage::Unsnooze(id, local_label_id) => {
                        let user_ctx = user_ctx.clone();
                        Command::command_from_future(async move {
                            Conversation::action_unsnooze(
                                user_ctx.action_queue(),
                                local_label_id,
                                id,
                            )
                            .await
                            .context("Failed to unsnooze conversation")
                            .map(|_| Command::None)
                        })
                    }
                    ConversationMessage::NextPage(conversations) => {
                        self.on_next_page(conversations)
                    }
                    ConversationMessage::ReplaceFrom(idx, conversations) => {
                        self.on_replace_from(idx, conversations)
                    }
                    ConversationMessage::ReplaceBefore(idx, conversations) => {
                        self.on_replace_before(idx, conversations)
                    }
                    ConversationMessage::ReplaceRange(from, to, conversations) => {
                        self.on_replace_range(from, to, conversations)
                    }
                    ConversationMessage::HasMore => {
                        let paginator = self.paginator.clone_inner();
                        Command::task(async move {
                            let has_more = paginator.has_more().await.unwrap();
                            let seen = paginator.seen().await.unwrap();
                            let synced = paginator.synced().await.unwrap();
                            let total = paginator.total().await.unwrap();
                            Command::message(Messages::DisplayInfo(
                                Some("Has more".to_owned()),
                                format!("Loaded: {seen}/{synced}/{total}, Has more: {has_more}"),
                            ))
                        })
                    }
                    ConversationMessage::DeleteAll(id) => delete_all(user_ctx.to_owned(), id),
                    _ => Command::None,
                }
            }

            MessagesStatus::Loading(_) => match message {
                Message::ConversationState(ConversationMessage::OpenSuccess(state)) => {
                    self.messages = MessagesStatus::Ready(state);
                    Command::None
                }
                Message::ConversationState(ConversationMessage::OpenFailed(e)) => {
                    self.messages = MessagesStatus::None;
                    Command::message(Messages::DisplayError(None, e))
                }
                _ => Command::None,
            },

            MessagesStatus::Ready(state) => {
                if let Message::ConversationState(ConversationMessage::Close) = &message {
                    self.close_conversation();
                    return Command::None;
                }
                state.update(user_ctx, message, mbox)
            }
        }
    }

    pub fn view(&mut self, frame: &mut Frame, mut area: Rect) {
        let mut banner = None;

        if let Some(AutoDeleteBanner { state, folder }) = self.autodelete_banner {
            banner = Some(match state {
                AutoDeleteState::AutoDeleteUpsell => format!(
                    "> Upgrade to automatically remove emails that have been in {folder} for over 30 days."
                ),
                AutoDeleteState::AutoDeleteDisabled => format!(
                    "> Auto-delete is off. Messages in {folder} will remain until you delete them manually."
                ),
                AutoDeleteState::AutoDeleteEnabled => {
                    format!("> Messages in {folder} will be automatically deleted after 30 days.")
                }
            });
        } else if self.paginator.supports_include_filter() {
            banner = Some(if self.include.has_spam_and_trash() {
                "> Seeing too many messages? [E]xclude Spam/Trash.".into()
            } else {
                "> Can't find what you're looking for? [I]nclude Spam/Trash.".into()
            });
        }

        if let Some(banner) = banner {
            let banner = Paragraph::new(banner).cyan();
            let banner_area;

            [banner_area, area] =
                Layout::vertical([Constraint::Length(2), Constraint::Fill(1)]).areas(area);

            frame.render_widget(banner, banner_area);
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
        if let MessagesStatus::Ready(message_state) = &self.messages {
            message_state.help_options(vec);
        } else {
            vec.push(("E", "Permanently delete all messages here"));
            vec.push(("z", "Snooze a conversation"));
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
    ids: Vec<LocalConversationId>,
) -> Command<Messages> {
    let local_label_id = mailbox.label_id();
    Command::task(async move {
        match Conversation::action_mark_read(ctx.action_queue(), local_label_id, ids).await {
            Ok(()) => Command::None,
            Err(e) => {
                let e = anyhow!("Failed to mark conversation as read: {e}");
                tracing::error!("{e:?}");
                Command::message(e)
            }
        }
    })
}

fn mark_conversation_unread(
    ctx: Arc<MailUserContext>,
    mailbox: &Mailbox,
    ids: Vec<LocalConversationId>,
) -> Command<Messages> {
    let current_label_id = mailbox.label_id();
    Command::task(async move {
        match Conversation::action_mark_unread(ctx.action_queue(), current_label_id, ids).await {
            Ok(()) => Command::None,
            Err(e) => {
                let e = anyhow!("Failed to mark conversation as read: {e}");
                tracing::error!("{e:?}");
                Command::message(e)
            }
        }
    })
}

fn delete_conversation(
    ctx: Arc<MailUserContext>,
    mailbox: &Mailbox,
    ids: Vec<LocalConversationId>,
) -> Command<Messages> {
    let current_label_id = mailbox.label_id();
    Command::message(Messages::raise_popup(
        YesNoPopup::new(
            "Confirm Conversation Delete",
            "Are you sure you wish to permanently delete the currently selected conversation?",
        )
        .on_accept(Command::task(async move {
            match Conversation::action_mark_deleted(ctx.action_queue(), current_label_id, ids).await
            {
                Ok(_) => Command::None,
                Err(e) => {
                    let e = anyhow!("Failed to delete conversation: {e}");
                    tracing::error!("{e:?}");
                    Command::message(e)
                }
            }
        })),
    ))
}

fn move_conversation(
    ctx: Arc<MailUserContext>,
    ids: Vec<LocalConversationId>,
    label_id: LocalLabelId,
) -> Command<Messages> {
    Command::task(async move {
        // TODO: refactor into common undo toast
        match async {
            let tether = ctx.user_stash().connection().await?;
            Conversation::action_move(&tether, ctx.action_queue(), label_id, ids).await
        }
        .await
        {
            Ok(None) => Command::None,
            Ok(Some(undo)) => {
                let ctx = ctx.clone();
                let popup = YesNoPopup::new(
                    "Undo move?",
                    "Moved successfully, would you like to undo this operation?",
                )
                .on_accept(Command::batch([
                    Command::message(Messages::DisplayBackgroundProgress(
                        "Cancelling Send".to_owned(),
                    )),
                    Command::task(async move {
                        if let Err(e) = async {
                            let mut tether = ctx.user_stash().connection().await?;
                            undo.undo(ctx.action_queue(), &mut tether)
                                .await
                                .context("Error undoing conversation labelling")
                        }
                        .await
                        {
                            Command::message(e)
                        } else {
                            Command::None
                        }
                    }),
                    Command::message(Messages::DismissBackgroundProgress),
                ]));
                Messages::raise_popup(popup).into()
            }
            Err(e) => {
                let e = anyhow!("Failed to move conversation: {e}");
                tracing::error!("{e:?}");
                e.into()
            }
        }
    })
}

fn star_conversation(
    ctx: Arc<MailUserContext>,
    ids: Vec<LocalConversationId>,
) -> Command<Messages> {
    Command::task(async move {
        match Conversation::action_star(ctx.action_queue(), ids).await {
            Ok(_) => Command::None,
            Err(e) => {
                let e = anyhow!("Failed to label conversation: {e}");
                tracing::error!("{e:?}");
                Command::message(e)
            }
        }
    })
}

fn unstar_conversation(
    ctx: Arc<MailUserContext>,
    ids: Vec<LocalConversationId>,
) -> Command<Messages> {
    Command::task(async move {
        match Conversation::action_unstar(ctx.action_queue(), ids).await {
            Ok(_) => Command::None,
            Err(e) => {
                let e = anyhow!("Failed to label conversation: {e}");
                tracing::error!("{e:?}");
                Command::message(e)
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
    let ctx2 = ctx.clone();
    let f = async move {
        Conversation::action_label_as(
            &ctx2.user_stash().connection().await?,
            ctx2.action_queue(),
            source_label_id,
            conversation_ids,
            selected_label_ids,
            partially_selected_label_ids,
            must_archive,
        )
        .await
        .context("Failed to apply label to conversation")
    };
    // TODO: refactor into common undo toast
    Command::task(async move {
        match f.await {
            Ok(output) => {
                let Some(undo) = output.undo else {
                    return Command::None;
                };
                let ctx = ctx.clone();
                let popup = YesNoPopup::new(
                    "Undo Labeling?",
                    "Labelled successfully, would you like to undo this operation?",
                )
                .on_accept(Command::batch([
                    Command::message(Messages::DisplayBackgroundProgress(
                        "Cancelling Send".to_owned(),
                    )),
                    Command::task(async move {
                        if let Err(e) = async {
                            let mut tether = ctx.user_stash().connection().await?;
                            undo.undo(ctx.action_queue(), &mut tether)
                                .await
                                .context("Error undoing conversation labelling")
                        }
                        .await
                        {
                            Command::message(e)
                        } else {
                            Command::None
                        }
                    }),
                    Command::message(Messages::DismissBackgroundProgress),
                ]));
                Messages::raise_popup(popup).into()
            }
            Err(e) => {
                tracing::error!("{e:?}");
                Command::message(e)
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
            match async {
                let tether = ctx.user_stash().connection().await?;
                MailMessage::action_delete_all_in_label(ctx.action_queue(), id, &tether).await
            }
            .await
            {
                Ok(_) => Command::None,
                Err(e) => {
                    let e = anyhow!("Failed to delete all in label: {e}");
                    tracing::error!("{e:?}");
                    Command::message(e)
                }
            }
        })),
    ))
}
