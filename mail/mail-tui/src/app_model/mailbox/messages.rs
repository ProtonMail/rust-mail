use super::LabelAs;
use super::search::SearchStatusBar;
use crate::CLI_ARGS;
use crate::app::Command;
use crate::app_model::mailbox::composer::Composer;
use crate::app_model::mailbox::paginator::Paginator;
use crate::app_model::mailbox::{ConversationMessage, ITEM_LIMIT, Items, Message, MessageMessage};
use crate::app_model::watcher::TuiWatchHandle;
use crate::app_model::{ChoosePopup, YesNoPopup};
use crate::messages::Messages;
use crate::widgets::utils::{date_from_timestamp, format_recipients, format_sender};
use crate::widgets::{
    CenteredThrobber, ScrollableParagraph, ScrollableParagraphState, ScrollableTable,
    ScrollableTableState,
};
use anyhow::{Context, Result, anyhow};
use futures::FutureExt;
use futures::future::try_join_all;
use itertools::Itertools as _;
use proton_calendar_api::CalendarAttendeeStatus;
use proton_calendar_common::{RsvpAnswerStatus, RsvpOccurrence, RsvpProgress};
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::os::safe_write;
use proton_mail_common::datatypes::message_banner::MessageBanner;
use proton_mail_common::datatypes::{
    ContextualConversation, LocalConversationId, LocalMessageId, MessageRecipientDisplayMode,
    ReadFilter, SearchOptions,
};
use proton_mail_common::decrypted_message::{DecryptedMessageBody, TransformOpts};
use proton_mail_common::draft::{Draft, ReplyMode};
use proton_mail_common::mail_scroller::{MailScroller, ScrollerUpdate};
use proton_mail_common::models::default_location::IncomingDefaultLocation;
use proton_mail_common::models::{Attachment, LabelWithCounters, Message as MailMessage};
use proton_mail_common::proton_mail_api::proton_core_api::services::proton::PrivateEmail;
use proton_mail_common::rsvp::RsvpEvent;
use proton_mail_common::{AppError, MailContextResult, MailUserContext, Mailbox};
use proton_mail_html_transformer::Html2TextOptions;
use ratatui::Frame;
use ratatui::crossterm::event::{Event, KeyCode, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table};
use stash::orm::Model;
use stash::stash::Tether;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{iter, thread};
use throbber_widgets_tui::ThrobberState;
use tokio::{fs, task};
use tracing::{debug, warn};

/// Displays a list of messages based of message metadata. If a conversation is opened the message
/// body will be displayed.
pub struct MessagesState {
    messages: Vec<MailMessage>,
    table_state: ScrollableTableState,
    open_message: DecryptedMessageStatus,
    mode: Mode,
    recipient_display_mode: MessageRecipientDisplayMode,
    ready: AtomicBool,
}

#[allow(dead_code)] // Watcher handle is needed to keep state
enum Mode {
    Label(Paginator),
    Search(Paginator),
    Conversation(TuiWatchHandle),
}

fn handle_scroller_update(update: ScrollerUpdate<MailMessage>) -> Messages {
    match update {
        ScrollerUpdate::Append { src: _, items } => MessageMessage::NextPage(items).into(),
        ScrollerUpdate::ReplaceFrom { src: _, idx, items } => {
            MessageMessage::ReplaceFrom(idx, items).into()
        }
        ScrollerUpdate::ReplaceBefore { src: _, idx, items } => {
            MessageMessage::ReplaceBefore(idx, items).into()
        }
        ScrollerUpdate::Error { src, error } => {
            let e = anyhow!("Message Reload Query src: {src:?}, error: {error}");
            tracing::error!("{e:?}");
            e.into()
        }
    }
}

const MESSAGE_DISPLAY_SIZE: u16 = 100;
const MIN_LIST_DISPLAY_SIZE: u16 = 20;
impl MessagesState {
    pub(super) fn build(
        ctx: Arc<MailUserContext>,
        mbox: Mailbox,
        label: LabelWithCounters,
        filter: ReadFilter,
    ) -> Command<Messages> {
        let label_id = mbox.label_id();
        let recipient_display_mode = mbox.recipient_display_mode();
        Command::task(async move {
            match Self::new_impl(ctx, label_id, filter, recipient_display_mode).await {
                Ok((state, background_command)) => Command::batch([
                    Command::message(Message::OpenMessageView(mbox, label, state).into()),
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
        recipient_display_mode: MessageRecipientDisplayMode,
    ) -> MailContextResult<(Self, Command<Messages>)> {
        let (scroller, handle) =
            MailScroller::messages(ctx.as_weak(), label_id, filter, ITEM_LIMIT).await?;
        let (paginator, command) =
            Paginator::new::<MailMessage>(scroller, handle, handle_scroller_update);

        paginator.next_page_command();
        let messages = vec![];

        Ok((
            Self {
                messages,
                table_state: ScrollableTableState::new(Some(0)),
                open_message: DecryptedMessageStatus::None,
                mode: Mode::Label(paginator),
                recipient_display_mode,
                ready: AtomicBool::new(false),
            },
            command,
        ))
    }

    pub(super) fn from_search(
        ctx: Arc<MailUserContext>,
        mbox: Mailbox,
        search_phrase: String,
    ) -> Command<Messages> {
        Command::task(async move {
            match Self::from_search_impl(ctx, search_phrase).await {
                Ok((state, background_command)) => Command::batch([
                    Command::message(Message::OpenSearchView(mbox, state).into()),
                    background_command,
                ]),
                Err(e) => Command::message(e.into()),
            }
        })
    }

    pub fn label_paginator(&self) -> Option<&Paginator> {
        if let Mode::Label(paginator) = &self.mode {
            Some(paginator)
        } else {
            None
        }
    }

    async fn from_search_impl(
        ctx: Arc<MailUserContext>,
        search_phrase: String,
    ) -> MailContextResult<(Self, Command<Messages>)> {
        let (scroller, handle) = MailScroller::search(
            ctx.as_weak(),
            SearchOptions::from(search_phrase.clone()),
            ITEM_LIMIT,
        )
        .await?;

        let (paginator, command) =
            Paginator::new::<MailMessage>(scroller, handle, handle_scroller_update);

        paginator.next_page_command();

        let messages = vec![];
        let total = paginator.total().await;

        Ok((
            Self {
                messages,
                table_state: ScrollableTableState::new(Some(0)),
                open_message: DecryptedMessageStatus::None,
                mode: Mode::Search(paginator),
                recipient_display_mode: MessageRecipientDisplayMode::Sender,
                ready: AtomicBool::new(false),
            },
            Command::batch(vec![
                Command::message(
                    Message::SearchStatusBar(SearchStatusBar {
                        search_phrase,
                        total,
                    })
                    .into(),
                ),
                command,
            ]),
        ))
    }

    pub(super) fn from_conversation(
        ctx: Arc<MailUserContext>,
        mbox: &Mailbox,
        conversation_id: LocalConversationId,
    ) -> Command<Messages> {
        let label_id = mbox.label_id();
        Command::task(async move {
            match Self::from_conversation_impl(ctx, label_id, conversation_id).await {
                Ok((state, background_command)) => Command::batch([
                    Command::message(ConversationMessage::OpenSuccess(Box::new(state)).into()),
                    background_command,
                ]),
                Err(e) => {
                    let e = anyhow!("Failed to open conversation {conversation_id}: {e}");
                    tracing::error!("{e:?}");
                    Command::message(ConversationMessage::OpenFailed(e).into())
                }
            }
        })
    }

    async fn from_conversation_impl(
        ctx: Arc<MailUserContext>,
        label_id: LocalLabelId,
        conversation_id: LocalConversationId,
    ) -> MailContextResult<(Self, Command<Messages>)> {
        let Some(conv_and_messages) = ContextualConversation::conversation_and_messages(
            conversation_id,
            label_id,
            ctx.user_stash(),
            ctx.session(),
        )
        .await?
        else {
            return Err(AppError::ConversationNotFound(conversation_id).into());
        };

        let handle = ContextualConversation::watch(ctx.user_stash())?;
        let (watcher, background_command) =
            TuiWatchHandle::from_watcher_handle(handle, move || {
                let tether = ctx.user_stash().connection();
                async move {
                    Some(
                        match MailMessage::in_conversation(conversation_id, &tether).await {
                            Ok(m) => MessageMessage::Refreshed(m).into(),
                            Err(e) => {
                                let e = anyhow!("Message list Query error: {e}");
                                tracing::error!("{e:?}");
                                e.into()
                            }
                        },
                    )
                }
                .boxed()
            });

        let index = conv_and_messages
            .messages
            .iter()
            .position(|m| m.id() == conv_and_messages.message_id_to_open)
            .unwrap_or(0);

        Ok((
            Self {
                messages: conv_and_messages.messages,
                table_state: ScrollableTableState::new(Some(index)),
                open_message: DecryptedMessageStatus::None,
                mode: Mode::Conversation(watcher),
                recipient_display_mode: MessageRecipientDisplayMode::Sender,
                ready: AtomicBool::new(false),
            },
            background_command,
        ))
    }

    pub fn open_message_body(&mut self, ctx: Arc<MailUserContext>) -> Command<Messages> {
        let Some(metadata) = self.selected_message() else {
            tracing::warn!("No message selected");
            return Command::None;
        };

        self.open_message = DecryptedMessageStatus::Loading(ThrobberState::default());

        Command::task(async {
            #[allow(clippy::redundant_closure_call)] // Poor's man try blocks
            let c: Result<_> = (|| async move {
                let stash = ctx.user_stash();
                let tether = stash.connection();
                let local_id = metadata.id();

                let decrypted = MailMessage::message_body(&ctx, local_id)
                    .await
                    .context("Failed to get message body")?;

                Ok(Box::new(
                    DecryptedMessage::new(metadata, decrypted, &ctx, tether).await?,
                ))
            })()
            .await;

            Command::message(MessageMessage::OpenBodyResult(c).into())
        })
    }

    fn display_message(&mut self, message: Result<Box<DecryptedMessage>>) {
        self.open_message = match message {
            Ok(message) => DecryptedMessageStatus::Success(message),
            Err(e) => DecryptedMessageStatus::Error(e),
        }
    }

    fn close_message(&mut self) {
        self.open_message = DecryptedMessageStatus::None;
    }

    fn selected_message(&self) -> Option<MailMessage> {
        let index = self.table_state.selected()?;
        self.messages.get(index).cloned()
    }

    fn selected_id(&self) -> Option<LocalMessageId> {
        let index = self.table_state.selected()?;
        self.messages.get(index).map(Model::id)
    }

    fn selected_id_and(
        &self,
        and: impl Fn(LocalMessageId) -> Command<Messages>,
    ) -> Command<Messages> {
        let Some(idx) = self.table_state.selected() else {
            return Command::none();
        };
        and(self.messages[idx].id())
    }

    fn msgs(&mut self) -> Vec<LocalMessageId> {
        self.table_state
            .take_selected_items(&|idx| self.messages[idx].id())
    }

    fn selected_email(&self) -> Option<PrivateEmail> {
        let index = self.table_state.selected()?;
        self.messages.get(index).map(|c| c.sender.address.clone())
    }

    fn try_select_non_empty_list(&mut self) {
        if !self.ready.load(Ordering::Acquire) {
            self.ready.store(true, Ordering::Release);
            self.table_state.select(0);
        } else if self.messages.is_empty() {
            self.ready.store(false, Ordering::Release);
        }
    }
}

impl MessagesState {
    #[allow(clippy::too_many_lines)]
    pub fn handle_event(
        &mut self,
        ctx: &Arc<MailUserContext>,
        mbox: &Mailbox,
        event: &Event,
    ) -> Command<Messages> {
        let Event::Key(key) = event else {
            return Command::None;
        };

        if matches!(self.mode, Mode::Search(_))
            && matches!(self.open_message, DecryptedMessageStatus::None)
            && key.code == KeyCode::Esc
        {
            return Command::batch(vec![
                Command::message(Message::ClearSearchStatusBar.into()),
                // TODO: For now its hard to go back in the previous state - fixme
                Command::message(Message::Sync(mbox.clone()).into()),
            ]);
        }

        if matches!(
            self.open_message,
            DecryptedMessageStatus::Success(_) | DecryptedMessageStatus::Error(_)
        ) && key.code == KeyCode::Esc
        {
            return Command::message(MessageMessage::CloseBody.into());
        }

        if let DecryptedMessageStatus::Success(state) = &mut self.open_message {
            match key.code {
                KeyCode::Char('k') | KeyCode::Up => {
                    if key.modifiers.intersects(KeyModifiers::SHIFT) {
                        state.content_scroll.scroll_up();
                        return Command::None;
                    }
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    if key.modifiers.intersects(KeyModifiers::SHIFT) {
                        state.content_scroll.scroll_down();
                        return Command::None;
                    }
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Char('k') | KeyCode::Up => {
                self.table_state.prev();
                Command::None
            }

            KeyCode::Char('j') | KeyCode::Down => {
                self.table_state.next();

                if let Mode::Label(paginator) = &self.mode {
                    if self.table_state.selected().unwrap_or_default()
                        >= self.messages.len().saturating_sub(1)
                    {
                        return paginator.next_page_command();
                    }
                }

                if let Mode::Search(paginator) = &self.mode {
                    if self.table_state.selected().unwrap_or_default()
                        == self.messages.len().saturating_sub(1)
                    {
                        return paginator.next_page_command();
                    }
                }

                Command::None
            }
            KeyCode::Char(' ') => {
                self.table_state.toggle();
                Command::None
            }
            KeyCode::Char('g') => {
                self.table_state.mark_many(0..self.messages.len());
                Command::None
            }
            KeyCode::Char('G') => {
                self.table_state.unmark_many(0..self.messages.len());
                Command::None
            }
            KeyCode::F(3) => self.handle_download_attachments(ctx),

            KeyCode::Char('e') => self
                .selected_id()
                .map(|id| Composer::open(ctx.to_owned(), id))
                .unwrap_or_default(),

            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.selected_id_and(|id| Composer::reply(ctx.to_owned(), id, ReplyMode::Sender))
            }

            KeyCode::Char('r') => MessageMessage::MarkRead(self.msgs()).into(),

            KeyCode::Char('u') => MessageMessage::MarkUnread(self.msgs()).into(),

            KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.selected_id_and(|id| Composer::reply(ctx.to_owned(), id, ReplyMode::All))
            }

            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.selected_id_and(|id| Composer::reply(ctx.to_owned(), id, ReplyMode::Forward))
            }

            KeyCode::Char('f') => MessageMessage::Star(self.msgs()).into(),

            KeyCode::Char('F') => MessageMessage::Unstar(self.msgs()).into(),

            KeyCode::Char('d') => MessageMessage::DeletePermanently(self.msgs()).into(),

            KeyCode::Char('b') => self
                .selected_email()
                .map(|email| MessageMessage::BlockSender(email, BlockOrUnblock::Block).into())
                .unwrap_or_default(),

            KeyCode::Char('B') => self
                .selected_email()
                .map(|email| MessageMessage::BlockSender(email, BlockOrUnblock::Unblock).into())
                .unwrap_or_default(),

            KeyCode::Char('s') => Message::OpenLabelSelectPopup.into(),

            KeyCode::Char('m') => Message::OpenMoveItemsPopup(Items::Message(self.msgs())).into(),

            KeyCode::Char('l') => Message::OpenLabelItemPopup(Items::Message(self.msgs())).into(),

            KeyCode::Char('h') => MessageMessage::HasMore.into(),

            KeyCode::Enter => self.selected_id_and(|_| MessageMessage::OpenBody.into()),

            KeyCode::Char('z') => {
                self.selected_id_and(|id| MessageMessage::CancelScheduleSend(id).into())
            }

            KeyCode::Char('p') => {
                self.selected_id_and(|id| MessageMessage::ReportPhishing(id).into())
            }

            KeyCode::Char('A') => self.handle_answer_rsvp(ctx),

            _ => Command::None,
        }
    }

    fn handle_download_attachments(&self, ctx: &Arc<MailUserContext>) -> Command<Messages> {
        let user_ctx = ctx.to_owned();

        let message = self
            .selected_message()
            .expect("Should have a message selected");

        debug!(
            "Downloading the attachments for message {}",
            message.subject
        );

        let download = Command::task(async move {
            let all = message.attachments_metadata.into_iter().map(|mdata| {
                let user_ctx = Arc::clone(&user_ctx);

                async move {
                    Attachment::get_attachment(&user_ctx, mdata.local_id.unwrap())
                        .await
                        .map(|att| {
                            format!(
                                "{} -> {}",
                                att.attachment_metadata.filename,
                                att.data_path.display(),
                            )
                        })
                }
            });

            let tri = try_join_all(all)
                .await
                .context("Failed to download attachments");

            match tri {
                Ok(attatchments) => Command::message(Messages::DisplayInfo(
                    Some("Attachments Successfully Fetched".to_owned()),
                    format!(
                        "{} attachments fetched successfully:\n{}",
                        attatchments.len(),
                        attatchments.join("\n"),
                    ),
                )),
                Err(e) => Command::message(Messages::DisplayError(None, e)),
            }
        });

        Command::batch([
            Command::message(Messages::DisplayBackgroundProgress(
                "Fetching attachments".to_string(),
            )),
            download,
        ])
    }

    fn handle_answer_rsvp(&self, ctx: &Arc<MailUserContext>) -> Command<Messages> {
        let DecryptedMessageStatus::Success(state) = &self.open_message else {
            return Command::None;
        };

        let Rsvp::Success(rsvp) = &state.rsvp else {
            return Command::None;
        };

        if rsvp.intent.is_reminder() {
            // Reminders can't be answered
            return Command::None;
        }

        let ctx = ctx.clone();
        let mut rsvp = rsvp.clone();

        Command::message(Messages::raise_popup(
            ChoosePopup::default()
                .with(
                    KeyCode::Char('y'),
                    "Answer: yes",
                    Some(RsvpAnswerStatus::Yes),
                )
                .with(
                    KeyCode::Char('m'),
                    "Answer: maybe",
                    Some(RsvpAnswerStatus::Maybe),
                )
                .with(KeyCode::Char('n'), "Answer: no", Some(RsvpAnswerStatus::No))
                .space()
                .with(KeyCode::Esc, "Go back", None)
                .on_reply(move |status| match status {
                    Some(status) => Command::batch([
                        Command::message(Messages::DismissPopup),
                        Command::message(Messages::DisplayBackgroundProgress(
                            "Answering invitation...".into(),
                        )),
                        Command::task(async move {
                            let mut tether = ctx.user_stash().connection();

                            let result = rsvp
                                .answer(&ctx, &mut tether, status)
                                .await
                                .context("Couldn't answer the invitation");

                            match result {
                                Ok(()) => {
                                    let status = match status {
                                        RsvpAnswerStatus::Yes => "Invitation accepted",
                                        RsvpAnswerStatus::Maybe => {
                                            "Invitation tentatively accepted"
                                        }
                                        RsvpAnswerStatus::No => "Invitation declined",
                                    };

                                    Command::batch([
                                        Command::message(Messages::Mailbox(Message::MessageState(
                                            MessageMessage::UpdateRsvp(rsvp),
                                        ))),
                                        Command::message(Messages::DismissBackgroundProgress),
                                        Command::message(Messages::DisplayInfo(
                                            None,
                                            status.into(),
                                        )),
                                    ])
                                }

                                Err(err) => Command::batch([
                                    Command::message(Messages::DismissBackgroundProgress),
                                    Command::message(Messages::DisplayError(None, err)),
                                ]),
                            }
                        }),
                    ]),

                    None => Command::message(Messages::DismissPopup),
                }),
        ))
    }

    #[allow(clippy::too_many_lines)]
    pub fn update(
        &mut self,
        user_ctx: &Arc<MailUserContext>,
        message: Message,
        mbox: &Mailbox,
    ) -> Command<Messages> {
        let Message::MessageState(message) = message else {
            return Command::None;
        };

        match message {
            MessageMessage::OpenBody => {
                return self.open_message_body(user_ctx.to_owned());
            }
            MessageMessage::OpenBodyResult(r) => {
                self.display_message(r);
            }
            MessageMessage::CloseBody => {
                self.close_message();
            }
            MessageMessage::Refreshed(messages) => {
                self.messages = messages;
            }
            MessageMessage::ReplaceFrom(idx, messages) => {
                self.messages.splice(idx.., messages);
                self.try_select_non_empty_list();
            }
            MessageMessage::ReplaceBefore(idx, messages) => {
                self.messages.splice(..idx, messages);
                self.try_select_non_empty_list();
            }
            MessageMessage::DeletePermanently(id) => {
                return delete_messages(user_ctx.to_owned(), mbox, id);
            }
            MessageMessage::MoveTo(msg_id, id) => {
                return move_message(user_ctx.to_owned(), mbox, msg_id, id);
            }
            MessageMessage::LabelAs(label_as) => {
                return label_message(user_ctx.to_owned(), *label_as);
            }
            MessageMessage::MarkRead(id) => {
                return mark_message_read(user_ctx.to_owned(), id);
            }
            MessageMessage::MarkUnread(id) => {
                return mark_message_unread(user_ctx.to_owned(), id);
            }
            MessageMessage::Star(id) => {
                return star_message(user_ctx.to_owned(), id);
            }
            MessageMessage::BlockSender(id, action) => {
                return block_sender(user_ctx.to_owned(), id, action);
            }
            MessageMessage::Unstar(id) => {
                return unstar_message(user_ctx.to_owned(), id);
            }
            MessageMessage::ReportPhishing(id) => {
                let ctx = user_ctx.to_owned();
                let popup = YesNoPopup::new(
                    "Confirm phishing report",
                    "Reporting a message as a phishing atempt will send the message to us, so we can analyze it and improve our filters. This means that we will be able to see the contents of the message in full.",
                )
                .on_accept(Command::from_future(async move {
                    MailMessage::action_report_phishing(ctx.action_queue(), id, &ctx.user_stash().connection())
                        .await
                        .context("Failed to star message")
                }));
                return Command::message(Messages::raise_popup(popup));
            }
            MessageMessage::NextPage(messages) => {
                self.messages.extend(messages);
                self.try_select_non_empty_list();
            }
            MessageMessage::HasMore => {
                if let Mode::Label(paginator) = &self.mode {
                    let paginator = paginator.clone_inner();
                    return Command::task(async move {
                        let has_more = paginator.has_more().await.unwrap();
                        let total = paginator.total().await.unwrap();
                        let seen = paginator.seen().await.unwrap();
                        Command::message(Messages::DisplayInfo(
                            Some("Has more".to_owned()),
                            format!("Loaded: {seen}/{total}, Has more: {has_more}"),
                        ))
                    });
                }
                if let Mode::Search(paginator) = &self.mode {
                    let paginator = paginator.clone_inner();
                    return Command::task(async move {
                        let has_more = paginator.has_more().await.unwrap();
                        let total = paginator.total().await.unwrap();
                        let seen = paginator.seen().await.unwrap();
                        Command::message(Messages::DisplayInfo(
                            Some("Has more".to_owned()),
                            format!("Loaded: {seen}/{total}, Has more: {has_more}"),
                        ))
                    });
                }
            }
            MessageMessage::CancelScheduleSend(id) => {
                return cancel_scheduled_send(user_ctx.to_owned(), id);
            }
            MessageMessage::UpdateRsvp(rsvp) => {
                if let DecryptedMessageStatus::Success(msg) = &mut self.open_message {
                    msg.rsvp = Rsvp::Success(rsvp);
                }
            }
        }
        Command::None
    }

    pub fn view(&mut self, frame: &mut Frame, area: Rect) {
        let table_area = self.open_message.draw(frame, area);

        if let Some(table_area) = table_area {
            let table = crate::widgets::messages::message_as_table(
                &self.messages,
                self.recipient_display_mode,
            );

            let scrollable_table = ScrollableTable::new(table, self.messages.len());

            frame.render_stateful_widget(scrollable_table, table_area, &mut self.table_state);
        }
    }

    pub fn help_options(&self, vec: &mut Vec<(&'static str, &'static str)>) {
        if matches!(self.open_message, DecryptedMessageStatus::Success(_)) {
            vec.extend_from_slice(&[
                ("Shift + ▲ ", "Scroll up in a message"),
                ("Shift + ▼ ", "Scroll down in a message"),
            ]);
        }
        vec.extend_from_slice(&[
            ("esc", "Close message"),
            ("F3", "Download all attachments"),
            ("A", "Answer RSVP"),
            ("e", "Open composer"),
            ("Ctrl + r", "Reply"),
            ("Ctrl + R", "Reply to all"),
            ("Ctrl + t", "Reply to all"),
            ("Ctrl + f", "Forward this message"),
            ("b/B", "block/unblock the sender of this message"),
        ]);
    }
}

pub struct DecryptedMessage {
    msg: MailMessage,
    content: String,
    content_scroll: ScrollableParagraphState,
    content_lines: usize,
    date: String,
    from: String,
    to: String,
    cc: String,
    bcc: String,
    labels: String,
    banners: Vec<MessageBanner>,
    rsvp: Rsvp,
}

enum Rsvp {
    None,
    Loading(task::JoinHandle<Result<Option<RsvpEvent>, String>>),
    Success(Box<RsvpEvent>),
    Error(String),
}

impl Rsvp {
    fn tick(&mut self) {
        if let Rsvp::Loading(task) = self {
            match task.now_or_never() {
                Some(Ok(Ok(Some(rsvp)))) => {
                    *self = Rsvp::Success(Box::new(rsvp));
                }
                Some(Ok(Ok(None))) => {
                    *self = Rsvp::None;
                }
                Some(Ok(Err(err))) => {
                    *self = Rsvp::Error(err.to_string());
                }
                Some(Err(err)) => {
                    *self = Rsvp::Error(err.to_string());
                }
                None => {
                    // Still loading
                }
            }
        }
    }
}

enum DecryptedMessageStatus {
    None,
    Loading(ThrobberState),
    Success(Box<DecryptedMessage>),
    Error(anyhow::Error),
}

impl DecryptedMessageStatus {
    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Option<Rect> {
        let (list_area, box_area, message_area) =
            if area.width <= MESSAGE_DISPLAY_SIZE + MIN_LIST_DISPLAY_SIZE {
                (None, Rect::default(), area)
            } else {
                let [list_area, box_area, message_area] = Layout::horizontal([
                    Constraint::Percentage(100),
                    Constraint::Length(1),
                    Constraint::Length(MESSAGE_DISPLAY_SIZE),
                ])
                .areas(area);
                (Some(list_area), box_area, message_area)
            };

        match self {
            DecryptedMessageStatus::None => return Some(area),
            DecryptedMessageStatus::Loading(state) => {
                frame.render_stateful_widget(
                    CenteredThrobber::default_with_label("Loading Message..."),
                    message_area,
                    state,
                );
            }
            DecryptedMessageStatus::Success(state) => {
                frame.render_widget(Block::new().borders(Borders::LEFT), box_area);
                state.draw(frame, message_area);
            }
            DecryptedMessageStatus::Error(e) => {
                frame.render_widget(Block::new().borders(Borders::LEFT), box_area);
                frame.render_widget(Text::from(e.to_string()), message_area);
            }
        }

        list_area
    }
}

impl DecryptedMessage {
    pub async fn new(
        msg: MailMessage,
        body: DecryptedMessageBody,
        ctx: &Arc<MailUserContext>,
        mut tether: Tether,
    ) -> Result<Self> {
        let sender = msg.sender.address.clone();

        let body_output = body
            .transformed(&sender, TransformOpts::default(), &tether)
            .await;

        if let Some(cmd_name) = CLI_ARGS.browser.as_deref() {
            let cmd_name = if !cmd_name.is_empty() {
                cmd_name
            } else if cfg!(target_os = "linux") {
                "xdg-open"
            } else if cfg!(target_os = "macos") {
                "open"
            } else {
                panic!("Please specify a browser in --browser");
            };

            let mut temp_dir = CLI_ARGS
                .html_dir
                .clone()
                .unwrap_or_else(|| std::env::temp_dir().join("proton_htmls"));

            let escaped_subject = PathBuf::from(
                &msg.subject
                    .replace(|c: char| !c.is_ascii_alphanumeric(), "_"),
            );

            temp_dir.push(escaped_subject);

            fs::create_dir_all(&temp_dir).await.unwrap();
            let before = temp_dir.join("before.html");
            fs::write(&before, &body.body).await.unwrap();

            let after = temp_dir.join("after.html");
            safe_write(&after, &body_output.body).unwrap();

            thread::spawn(move || {
                std::process::Command::new(cmd_name)
                    .args([&after])
                    .spawn()
                    .unwrap()
                    .wait()
                    .unwrap();
            });
        }

        let content = html_to_text(&body_output.body)?;
        let content_scroll = ScrollableParagraphState::new();
        let content_lines = content.chars().filter(|c| *c == '\n').count();

        let date = date_from_timestamp(msg.time);
        let from = format_sender(&msg.sender);
        let to = format_recipients(&msg.to_list);
        let cc = format_recipients(&msg.cc_list);
        let bcc = format_recipients(&msg.bcc_list);
        let labels = msg.custom_labels.iter().map(|l| &l.name).join(", ");

        let rsvp = match body.identify_rsvp(ctx).await {
            Ok(Some(rsvp)) => {
                let task = task::spawn({
                    let ctx = (*ctx).clone();

                    async move {
                        rsvp.fetch(&ctx, &mut tether)
                            .await
                            .map_err(|err| format!("Couldn't fetch RSVP: {err}"))
                            .inspect_err(|err| warn!("{err}"))
                    }
                });

                Rsvp::Loading(task)
            }

            Ok(None) => Rsvp::None,
            Err(err) => Rsvp::Error(err.to_string()),
        };

        Ok(Self {
            msg,
            content,
            content_scroll,
            content_lines,
            date,
            from,
            to,
            cc,
            bcc,
            labels,
            banners: body_output.body_banners,
            rsvp,
        })
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect) {
        let [headers_area, banners_area, rsvp_area, content_area] = Layout::vertical([
            Constraint::Length(Self::lay_headers()),
            Constraint::Length(self.lay_banners()),
            Constraint::Length(self.lay_rsvp()),
            Constraint::Fill(1),
        ])
        .areas(area);

        self.rsvp.tick();
        self.draw_headers(frame, headers_area);
        self.draw_banners(frame, banners_area);
        self.draw_rsvp(frame, rsvp_area);
        self.draw_content(frame, content_area);
    }

    fn lay_headers() -> u16 {
        7
    }

    fn draw_headers(&self, frame: &mut Frame, area: Rect) {
        let headers = vec![
            Row::new([
                Cell::from("Subject:"),
                Cell::from(self.msg.subject.as_str()),
            ])
            .bold(),
            Row::new([Cell::from("Date:").bold(), Cell::from(self.date.as_str())]),
            Row::new([Cell::from("From:").bold(), Cell::from(self.from.as_str())]),
            Row::new([Cell::from("To:").bold(), Cell::from(self.to.as_str())]),
            Row::new([Cell::from("CC:").bold(), Cell::from(self.cc.as_str())]),
            Row::new([Cell::from("BCC:").bold(), Cell::from(self.bcc.as_str())]),
            Row::new([
                Cell::from("Labels:").bold(),
                Cell::from(self.labels.as_str()),
            ]),
        ];

        let widths = [Constraint::Length(10), Constraint::Fill(1)];
        let table = Table::new(headers, widths).column_spacing(1);

        frame.render_widget(table, area);
    }

    fn lay_banners(&self) -> u16 {
        self.banners.len().try_into().unwrap()
    }

    fn draw_banners(&self, frame: &mut Frame, area: Rect) {
        let rows = self.banners.iter().map(|banner| match banner {
            // TODO: add mark ham to tui and hints here
            MessageBanner::BlockedSender => ListItem::from("You blocked this sender."),
            MessageBanner::PhishingAttempt { auto: true } => {
                ListItem::from("The system thinks that this is a phishing attempt")
            }
            MessageBanner::PhishingAttempt { auto: false } => {
                ListItem::from("You marked this as a phishing attempt")
            }
            MessageBanner::Spam { auto: true } => {
                ListItem::from("This message was automatically marked as spam")
            }
            MessageBanner::Spam { auto: false } => {
                ListItem::from("You marked this message as spam")
            }
            MessageBanner::Expiry { timestamp } => ListItem::from(format!(
                "This message will expire at {}",
                date_from_timestamp(*timestamp)
            )),

            #[allow(clippy::cast_possible_wrap)]
            MessageBanner::AutoDelete { timestamp } => ListItem::from(format!(
                "This message will auto-delete at {}",
                date_from_timestamp(*timestamp)
            )),

            MessageBanner::RemoteContent => ListItem::from(
                "This message contains remote images. Use the --browser flag to see them.",
            ),

            MessageBanner::EmbeddedImages => ListItem::from(
                "This message contains embedded images, which can't be shown in the TUI.",
            ),

            MessageBanner::ScheduledSend { timestamp } => ListItem::from(format!(
                "This message will be sent at {}",
                date_from_timestamp(*timestamp)
            )),

            _ => ListItem::from("unimplemented"),
        });

        frame.render_widget(List::new(rows), area);
    }

    fn lay_rsvp(&self) -> u16 {
        match &self.rsvp {
            Rsvp::None => 0,
            Rsvp::Loading(_) => 2,

            Rsvp::Success(rsvp) => {
                let progress = match rsvp.progress {
                    RsvpProgress::Pending => 0,
                    RsvpProgress::Ongoing | RsvpProgress::Ended | RsvpProgress::Cancelled => 2,
                };

                let header = 4;
                let organizer = 1;
                let attendees = rsvp.attendees.len();

                progress + header + organizer + attendees
            }

            Rsvp::Error(msg) => 1 + msg.lines().count(),
        }
        .try_into()
        .unwrap()
    }

    fn draw_rsvp(&self, frame: &mut Frame, area: Rect) {
        if let Rsvp::None = &self.rsvp {
            return;
        }

        let [sep_area, body_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(area);

        frame.render_widget(Block::new().borders(Borders::TOP), sep_area);

        match &self.rsvp {
            Rsvp::None => {
                unreachable!();
            }
            Rsvp::Loading(_) => {
                Self::draw_rsvp_loading(frame, body_area);
            }
            Rsvp::Success(rsvp) => {
                Self::draw_rsvp_success(frame, body_area, rsvp);
            }
            Rsvp::Error(err) => {
                Self::draw_rsvp_error(frame, body_area, err);
            }
        }
    }

    fn draw_rsvp_loading(frame: &mut Frame, area: Rect) {
        frame.render_widget(Paragraph::new("Loading event..."), area);
    }

    fn draw_rsvp_success(frame: &mut Frame, area: Rect, rsvp: &RsvpEvent) {
        let rsvp_progress = match rsvp.progress {
            RsvpProgress::Pending => None,
            RsvpProgress::Ongoing => Some(Text::raw("~ Event is in progress").fg(Color::Yellow)),
            RsvpProgress::Ended => Some(Text::raw("~ Event has already ended").fg(Color::Yellow)),
            RsvpProgress::Cancelled => {
                Some(Text::raw("! Event has been cancelled").fg(Color::Yellow))
            }
        };

        let rsvp_progress = rsvp_progress
            .map(|txt| vec![txt, Text::raw("")])
            .unwrap_or_default();

        let fg = match rsvp.progress {
            RsvpProgress::Pending | RsvpProgress::Ongoing => Color::White,
            RsvpProgress::Ended | RsvpProgress::Cancelled => Color::DarkGray,
        };

        let rsvp_summary = rsvp.summary.as_deref().unwrap_or("(no title)");

        let rsvp_occur = {
            let when = match rsvp.occurrence {
                RsvpOccurrence::Date { starts_at, ends_at } if ends_at == starts_at => {
                    format!("{starts_at}")
                }
                RsvpOccurrence::Date { starts_at, ends_at } => {
                    format!("{starts_at} - {ends_at}")
                }
                RsvpOccurrence::DateTime { starts_at, ends_at } => {
                    format!("{starts_at} - {ends_at}")
                }
            };

            if let Some(loc) = &rsvp.location {
                format!("{when} @ {loc}")
            } else {
                when
            }
        };

        let rsvp_org = format!("- <{}> (organizer)", rsvp.organizer.email);

        let rsvp_atts = rsvp.attendees.iter().map(|att| {
            let status = match att.status {
                CalendarAttendeeStatus::Unanswered => "unanswered",
                CalendarAttendeeStatus::Maybe => "maybe",
                CalendarAttendeeStatus::No => "no",
                CalendarAttendeeStatus::Yes => "yes",
            };

            format!("- <{}> ({status})", att.email)
        });

        let rsvp_summary = Text::from(rsvp_summary).fg(fg);
        let rsvp_occur = Text::from(rsvp_occur).fg(fg);
        let rsvp_org = Text::from(rsvp_org).fg(fg);
        let rsvp_atts = rsvp_atts.map(|att| Text::from(att).fg(fg));

        let rows = rsvp_progress
            .into_iter()
            .chain(iter::once(rsvp_summary))
            .chain(iter::once(rsvp_occur))
            .chain(iter::once(Text::raw("")))
            .chain(iter::once(rsvp_org))
            .chain(rsvp_atts);

        frame.render_widget(List::new(rows), area);
    }

    fn draw_rsvp_error(frame: &mut Frame, area: Rect, err: &str) {
        frame.render_widget(Paragraph::new(err), area);
    }

    fn draw_content(&mut self, frame: &mut Frame, area: Rect) {
        let [sep_area, body_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(area);

        frame.render_widget(Block::new().borders(Borders::TOP), sep_area);

        // ---

        let para = Paragraph::new(&*self.content);
        let para = ScrollableParagraph::new(para, self.content_lines);

        frame.render_stateful_widget(para, body_area, &mut self.content_scroll);
    }
}

fn html_to_text(message: &str) -> Result<String> {
    // TODO: Best effort terminal image rendering. See https://docs.rs/termimage/latest/termimage/
    let cursor = std::io::Cursor::new(message);
    proton_mail_html_transformer::Transformer::html2text(cursor, Html2TextOptions::default())
        .map_err(|e| anyhow!("Failed to parse HTML: {e}"))
}

fn mark_message_read(ctx: Arc<MailUserContext>, ids: Vec<LocalMessageId>) -> Command<Messages> {
    Command::from_future(async move {
        MailMessage::action_mark_read(ctx.action_queue(), ids)
            .await
            .context("Failed to mark message as read")
    })
}

fn mark_message_unread(ctx: Arc<MailUserContext>, ids: Vec<LocalMessageId>) -> Command<Messages> {
    Command::from_future(async move {
        MailMessage::action_mark_unread(ctx.action_queue(), ids)
            .await
            .context("Failed to mark message as unread")
    })
}

fn delete_messages(
    ctx: Arc<MailUserContext>,
    mailbox: &Mailbox,
    ids: Vec<LocalMessageId>,
) -> Command<Messages> {
    let current_label_id = mailbox.label_id();
    Command::message(Messages::raise_popup(
        YesNoPopup::new(
            "Confirm Message Delete",
            "Are you sure you wish to permanently delete the currently selected message?",
        )
        .on_accept(Command::from_future(async move {
            MailMessage::action_delete(ctx.action_queue(), current_label_id, ids)
                .await
                .context("Failed to delete message: {e}")
                .map(|_| ())
        })),
    ))
}

fn star_message(ctx: Arc<MailUserContext>, ids: Vec<LocalMessageId>) -> Command<Messages> {
    Command::from_future(async move {
        MailMessage::action_star(ctx.action_queue(), ids)
            .await
            .context("Failed to star message")
            .map(|_| ())
    })
}

fn unstar_message(ctx: Arc<MailUserContext>, ids: Vec<LocalMessageId>) -> Command<Messages> {
    Command::from_future(async move {
        MailMessage::action_unstar(ctx.action_queue(), ids)
            .await
            .context("Failed to star message")
            .map(|_| ())
    })
}

fn label_message(
    ctx: Arc<MailUserContext>,
    LabelAs {
        source_label_id,
        item_ids: conversation_ids,
        selected_label_ids,
        partially_selected_label_ids,
        must_archive,
    }: LabelAs<LocalMessageId>,
) -> Command<Messages> {
    Command::from_future(async move {
        MailMessage::action_label_as(
            ctx.action_queue(),
            source_label_id,
            conversation_ids,
            selected_label_ids,
            partially_selected_label_ids,
            must_archive,
        )
        .await
        .context("Failed to apply label to message")
        .map(|_| ())
    })
}

fn move_message(
    ctx: Arc<MailUserContext>,
    mailbox: &Mailbox,
    ids: Vec<LocalMessageId>,
    label_id: LocalLabelId,
) -> Command<Messages> {
    let current_label_id = mailbox.label_id();
    Command::from_future(async move {
        MailMessage::action_move(ctx.action_queue(), current_label_id, label_id, ids)
            .await
            .context("Failed to move message")
            .map(|_| ())
    })
}

fn block_sender(
    ctx: Arc<MailUserContext>,
    email: PrivateEmail,
    block_or_unblock: BlockOrUnblock,
) -> Command<Messages> {
    Command::from_future(async move {
        match block_or_unblock {
            BlockOrUnblock::Block => {
                IncomingDefaultLocation::action_block(ctx.action_queue(), email)
                    .await
                    .context("Failed to block or unblock sender")
                    .map(|_| ())
            }
            BlockOrUnblock::Unblock => {
                IncomingDefaultLocation::action_unblock(ctx.action_queue(), email)
                    .await
                    .context("Failed to block or unblock sender")
                    .map(|_| ())
            }
        }
    })
}

pub enum BlockOrUnblock {
    Block,
    Unblock,
}

fn cancel_scheduled_send(ctx: Arc<MailUserContext>, id: LocalMessageId) -> Command<Messages> {
    Command::batch([
        Command::message(Messages::DisplayBackgroundProgress(
            "Canceling scheduled send".to_owned(),
        )),
        Command::task(async move {
            let cmd = match Draft::cancel_schedule_send(&ctx, id).await {
                Ok(_) => Composer::open(ctx, id),
                Err(e) => Command::message(Messages::DisplayError(
                    Some("Failed to cancel schedule send".to_owned()),
                    anyhow::Error::new(e),
                )),
            };

            Command::batch([Command::message(Messages::DismissBackgroundProgress), cmd])
        }),
    ])
}
