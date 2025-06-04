use super::LabelAs;
use super::search::SearchStatusBar;
use crate::CLI_ARGS;
use crate::app::Command;
use crate::app_model::YesNoPopup;
use crate::app_model::mailbox::composer::Composer;
use crate::app_model::mailbox::paginator::Paginator;
use crate::app_model::mailbox::{ConversationMessage, ITEM_LIMIT, Item, Message, MessageMessage};
use crate::app_model::watcher::WatchHandle;
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
use proton_calendar_common::{RsvpEvent, RsvpOccurrence};
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::os::safe_write;
use proton_mail_common::datatypes::message_banner::MessageBanner;
use proton_mail_common::datatypes::{
    ContextualConversation, LocalConversationId, LocalMessageId, MessageRecipientDisplayMode,
    ReadFilter, SearchOptions,
};
use proton_mail_common::decrypted_message::{DecryptedMessageBody, TransformOpts};
use proton_mail_common::draft::{Draft, ReplyMode};
use proton_mail_common::mail_scroller::{DataScrollerSource, MailScroller, SearchScrollerSource};
use proton_mail_common::models::default_location::IncomingDefaultLocation;
use proton_mail_common::models::{
    Attachment, LabelWithCounters, Message as MailMessage, MessageScrollData,
};
use proton_mail_common::{AppError, MailContextResult, MailUserContext, Mailbox};
use proton_mail_html_transformer::Html2TextOptions;
use ratatui::Frame;
use ratatui::crossterm::event::{Event, KeyCode, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table};
use stash::stash::{Tether, WatcherHandle};
use std::path::PathBuf;
use std::sync::Arc;
use std::{iter, thread};
use throbber_widgets_tui::ThrobberState;
use tokio::fs;
use tracing::debug;

/// Displays a list of messages based of message metadata. If a conversation is opened the message
/// body will be displayed.
pub struct MessagesState {
    messages: Vec<MailMessage>,
    table_state: ScrollableTableState,
    open_message: DecryptedMessageStatus,
    mode: Mode,
    recipient_display_mode: MessageRecipientDisplayMode,
}

#[allow(dead_code)] // Watcher handle is needed to keep state
enum Mode {
    Label(Paginator<DataScrollerSource<MessageScrollData>>),
    Search(Paginator<SearchScrollerSource>),
    Conversation(WatchHandle),
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
        let context = ctx.clone();
        let (paginator, command) = Paginator::new(
            || {
                async move {
                    MailScroller::messages(context.as_weak(), label_id, filter, ITEM_LIMIT).await
                }
                .boxed()
            },
            |result| match result {
                Ok(messages) => MessageMessage::Refreshed(messages).into(),
                Err(e) => {
                    let e = anyhow!("Message Reload Query error: {e}");
                    tracing::error!("{e:?}");
                    e.into()
                }
            },
        )
        .await?;

        let messages = paginator.fetch_more().await?;

        Ok((
            Self {
                messages,
                table_state: ScrollableTableState::new(Some(0)),
                open_message: DecryptedMessageStatus::None,
                mode: Mode::Label(paginator),
                recipient_display_mode,
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

    async fn from_search_impl(
        ctx: Arc<MailUserContext>,
        search_phrase: String,
    ) -> MailContextResult<(Self, Command<Messages>)> {
        let context = ctx.clone();
        let search_phrase_clone = search_phrase.clone();
        let (paginator, command) = Paginator::new(
            || {
                async move {
                    MailScroller::search(
                        context.as_weak(),
                        SearchOptions::from(search_phrase_clone),
                        ITEM_LIMIT,
                    )
                    .await
                }
                .boxed()
            },
            |result| match result {
                Ok(messages) => MessageMessage::Refreshed(messages).into(),
                Err(e) => {
                    let e = anyhow!("Message Reload Query error: {e}");
                    tracing::error!("{e:?}");
                    e.into()
                }
            },
        )
        .await?;

        let messages = paginator.fetch_more().await?;
        let total = paginator.total().await;

        Ok((
            Self {
                messages,
                table_state: ScrollableTableState::new(Some(0)),
                open_message: DecryptedMessageStatus::None,
                mode: Mode::Search(paginator),
                recipient_display_mode: MessageRecipientDisplayMode::Sender,
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
                    Command::message(
                        ConversationMessage::OpenConversationSuccess(Box::new(state)).into(),
                    ),
                    background_command,
                ]),
                Err(e) => {
                    let e = anyhow!("Failed to open conversation {conversation_id}: {e}");
                    tracing::error!("{e:?}");
                    Command::message(ConversationMessage::OpenConversationFailed(e).into())
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

        let WatcherHandle {
            handle, receiver, ..
        } = ContextualConversation::watch(ctx.user_stash())?;
        let (watcher, background_command) = WatchHandle::new(receiver, handle, move |()| {
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
            .position(|m| m.local_id.unwrap() == conv_and_messages.message_id_to_open)
            .unwrap_or(0);

        Ok((
            Self {
                messages: conv_and_messages.messages,
                table_state: ScrollableTableState::new(Some(index)),
                open_message: DecryptedMessageStatus::None,
                mode: Mode::Conversation(watcher),
                recipient_display_mode: MessageRecipientDisplayMode::Sender,
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
                let mut tether = stash.connection();
                let local_id = metadata.local_id.unwrap();

                let decrypted = MailMessage::message_body(&ctx, local_id)
                    .await
                    .context("Failed to get message body")?;

                Ok(Box::new(
                    DecryptedMessage::new(metadata, decrypted, &ctx, &mut tether).await?,
                ))
            })()
            .await;

            Command::message(MessageMessage::OpenMessageBodyResult(c).into())
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

    fn messages_refreshed(&mut self, messages: Vec<MailMessage>) {
        self.messages = messages;
    }

    fn selected_message(&self) -> Option<MailMessage> {
        let index = self.table_state.selected()?;
        self.messages.get(index).cloned()
    }

    fn selected_message_id(&self) -> Option<LocalMessageId> {
        let index = self.table_state.selected()?;
        self.messages.get(index).map(|c| c.local_id.unwrap())
    }

    fn selected_email(&self) -> Option<String> {
        let index = self.table_state.selected()?;
        self.messages.get(index).map(|c| c.sender.address.clone())
    }
}

impl MessagesState {
    #[allow(clippy::too_many_lines)]
    pub fn handle_event(
        &mut self,
        user_ctx: &Arc<MailUserContext>,
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
            return Command::message(MessageMessage::CloseMessageBody.into());
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
                        == self.messages.len().saturating_sub(1)
                    {
                        return paginator.next_page_command(|v| {
                            Command::message(MessageMessage::NextPage(v).into())
                        });
                    }
                }
                if let Mode::Search(paginator) = &self.mode {
                    if self.table_state.selected().unwrap_or_default()
                        == self.messages.len().saturating_sub(1)
                    {
                        return paginator.next_page_command(|v| {
                            Command::message(MessageMessage::NextPage(v).into())
                        });
                    }
                }
                Command::None
            }
            KeyCode::Char('a') => {
                let user_ctx = user_ctx.to_owned();

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
                                        "{} -> {:?}",
                                        att.attachment_metadata.filename, att.data_path,
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
            KeyCode::Char('e') => self
                .selected_message_id()
                .map(|id| Composer::open(user_ctx.to_owned(), id))
                .unwrap_or_default(),
            KeyCode::Char('u') => self
                .selected_message_id()
                .map(|id| Command::message(MessageMessage::MarkMessageUnread(id).into()))
                .unwrap_or_default(),
            KeyCode::Char('r') => self
                .selected_message_id()
                .map(|id| {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        if key.modifiers.contains(KeyModifiers::SHIFT) {
                            Composer::reply(user_ctx.to_owned(), id, ReplyMode::All)
                        } else {
                            Composer::reply(user_ctx.to_owned(), id, ReplyMode::Sender)
                        }
                    } else {
                        Command::message(MessageMessage::MarkMessageRead(id).into())
                    }
                })
                .unwrap_or_default(),
            KeyCode::Char('f') => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.selected_message_id()
                        .map(|id| Composer::reply(user_ctx.to_owned(), id, ReplyMode::Forward))
                        .unwrap_or_default()
                } else {
                    self.selected_message_id()
                        .map(|id| Command::message(MessageMessage::StarMessage(id).into()))
                        .unwrap_or_default()
                }
            }
            KeyCode::Char('F') => self
                .selected_message_id()
                .map(|id| Command::message(MessageMessage::UnstarMessage(id).into()))
                .unwrap_or_default(),
            KeyCode::Char('t') => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.selected_message_id()
                        .map(|id| Composer::reply(user_ctx.to_owned(), id, ReplyMode::All))
                        .unwrap_or_default()
                } else {
                    Command::None
                }
            }
            KeyCode::Char('d') => self
                .selected_message_id()
                .map(|id| Command::message(MessageMessage::DeleteMessage(id).into()))
                .unwrap_or_default(),
            KeyCode::Char('b') => self
                .selected_email()
                .map(|email| {
                    Command::message(
                        MessageMessage::BlockSender(email, BlockOrUnblock::Block).into(),
                    )
                })
                .unwrap_or_default(),
            KeyCode::Char('B') => self
                .selected_email()
                .map(|email| {
                    Command::message(
                        MessageMessage::BlockSender(email, BlockOrUnblock::Unblock).into(),
                    )
                })
                .unwrap_or_default(),
            KeyCode::Char('s') => Command::message(Message::OpenLabelSelectPopup.into()),
            KeyCode::Char('m') => self
                .selected_message_id()
                .map(|id| Command::message(Message::OpenMoveItemPopup(Item::Message(id)).into()))
                .unwrap_or_default(),
            KeyCode::Char('l') => self
                .selected_message_id()
                .map(|id| Command::message(Message::OpenLabelItemPopup(Item::Message(id)).into()))
                .unwrap_or_default(),
            KeyCode::Char('h') => Command::message(MessageMessage::HasMore.into()),
            KeyCode::Enter => self
                .selected_message_id()
                .map(|_| Command::message(MessageMessage::OpenMessageBody.into()))
                .unwrap_or_default(),
            KeyCode::Char('z') => self
                .selected_message_id()
                .map(|id| Command::message(MessageMessage::CancelScheduleSend(id).into()))
                .unwrap_or_default(),
            KeyCode::Char('p') => self
                .selected_message_id()
                .map(|id| Command::message(MessageMessage::ReportPhishing(id).into()))
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
        let Message::MessageState(message) = message else {
            return Command::None;
        };

        match message {
            MessageMessage::OpenMessageBody => {
                return self.open_message_body(user_ctx.to_owned());
            }
            MessageMessage::OpenMessageBodyResult(r) => {
                self.display_message(r);
            }
            MessageMessage::CloseMessageBody => {
                self.close_message();
            }
            MessageMessage::Refreshed(messages) => self.messages_refreshed(messages),
            MessageMessage::DeleteMessage(id) => {
                return delete_message(user_ctx.to_owned(), mbox, id);
            }
            MessageMessage::MoveMessage(msg_id, id) => {
                return move_message(user_ctx.to_owned(), mbox, msg_id, id);
            }
            MessageMessage::LabelMessage(label_as) => {
                return label_message(user_ctx.to_owned(), *label_as);
            }
            MessageMessage::MarkMessageRead(id) => {
                return mark_message_read(user_ctx.to_owned(), id);
            }
            MessageMessage::MarkMessageUnread(id) => {
                return mark_message_unread(user_ctx.to_owned(), id);
            }
            MessageMessage::StarMessage(id) => {
                return star_message(user_ctx.to_owned(), id);
            }
            MessageMessage::BlockSender(id, action) => {
                return block_sender(user_ctx.to_owned(), id, action);
            }
            MessageMessage::UnstarMessage(id) => {
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
            }
            MessageMessage::HasMore => {
                if let Mode::Label(paginator) = &self.mode {
                    let paginator_clone = paginator.clone_paginator();
                    return Command::task(async move {
                        let paginator = paginator_clone.lock().await;
                        let has_more = paginator.has_more().await.unwrap();
                        let total = paginator.total();
                        let seen = paginator.seen().await.unwrap();
                        Command::message(Messages::DisplayInfo(
                            Some("Has more".to_owned()),
                            format!("Loaded: {seen}/{total}, Has more: {has_more}"),
                        ))
                    });
                }
                if let Mode::Search(paginator) = &self.mode {
                    let paginator_clone = paginator.clone_paginator();
                    return Command::task(async move {
                        let paginator = paginator_clone.lock().await;
                        let has_more = paginator.has_more().await.unwrap();
                        let total = paginator.total();
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
            ("a", "Download all attachments"),
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
    metadata: MailMessage,
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
    rsvp: Option<Result<RsvpEvent, String>>,
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
        metadata: MailMessage,
        body: DecryptedMessageBody,
        ctx: &MailUserContext,
        tether: &mut Tether,
    ) -> Result<Self> {
        let body_output = body.transformed(TransformOpts::default(), tether).await;

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
                &metadata
                    .subject
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

        let date = date_from_timestamp(metadata.time);
        let from = format_sender(&metadata.sender);
        let to = format_recipients(&metadata.to_list);
        let cc = format_recipients(&metadata.cc_list);
        let bcc = format_recipients(&metadata.bcc_list);
        let labels = metadata.custom_labels.iter().map(|l| &l.name).join(", ");

        let rsvp = if let Some(rsvp) = metadata.rsvp_attachment_id() {
            metadata
                .fetch_rsvp(ctx, rsvp, tether)
                .await
                .map_err(|err| format!("Can't fetch RSVP: {err}"))
                .transpose()
        } else {
            None
        };

        Ok(Self {
            metadata,
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
                Cell::from(self.metadata.subject.as_str()),
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
            Some(Ok(rsvp)) => (3 + rsvp.attendees.len()).try_into().unwrap(),
            Some(Err(_)) => 2,
            None => 0,
        }
    }

    fn draw_rsvp(&self, frame: &mut Frame, area: Rect) {
        let Some(rsvp) = &self.rsvp else {
            return;
        };

        let [sep_area, body_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(area);

        frame.render_widget(Block::new().borders(Borders::TOP), sep_area);

        // ---

        let rsvp = match rsvp {
            Ok(rsvp) => rsvp,

            Err(err) => {
                frame.render_widget(Paragraph::new(err.as_str()), body_area);
                return;
            }
        };

        // ---

        let rsvp_occur = match rsvp.occurrence {
            RsvpOccurrence::Date { starts_at, ends_at } => {
                format!("{starts_at} - {ends_at}")
            }
            RsvpOccurrence::DateTime { starts_at, ends_at } => {
                format!("{starts_at} - {ends_at}")
            }
        };

        let rsvp_atts = rsvp.attendees.iter().map(|att| {
            let status = match att.status {
                CalendarAttendeeStatus::Unanswered => "unanswered",
                CalendarAttendeeStatus::Maybe => "maybe",
                CalendarAttendeeStatus::No => "no",
                CalendarAttendeeStatus::Yes => "yes",
            };

            format!("- <{}> ({status})", att.email)
        });

        let rows = iter::once(rsvp.title.clone())
            .chain(iter::once(rsvp_occur))
            .chain(iter::once(String::default()))
            .chain(rsvp_atts);

        frame.render_widget(List::new(rows), body_area);
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

fn mark_message_read(ctx: Arc<MailUserContext>, id: LocalMessageId) -> Command<Messages> {
    Command::from_future(async move {
        MailMessage::action_mark_read(ctx.action_queue(), vec![id])
            .await
            .context("Failed to mark message as read")
    })
}

fn mark_message_unread(ctx: Arc<MailUserContext>, id: LocalMessageId) -> Command<Messages> {
    Command::from_future(async move {
        MailMessage::action_mark_unread(ctx.action_queue(), vec![id])
            .await
            .context("Failed to mark message as unread")
    })
}

fn delete_message(
    ctx: Arc<MailUserContext>,
    mailbox: &Mailbox,
    id: LocalMessageId,
) -> Command<Messages> {
    let current_label_id = mailbox.label_id();
    Command::message(Messages::raise_popup(
        YesNoPopup::new(
            "Confirm Message Delete",
            "Are you sure you wish to permanently delete the currently selected message?",
        )
        .on_accept(Command::from_future(async move {
            MailMessage::action_delete(ctx.action_queue(), current_label_id, vec![id])
                .await
                .context("Failed to delete message: {e}")
                .map(|_| ())
        })),
    ))
}

fn star_message(ctx: Arc<MailUserContext>, id: LocalMessageId) -> Command<Messages> {
    Command::from_future(async move {
        MailMessage::action_star(ctx.action_queue(), vec![id])
            .await
            .context("Failed to star message")
            .map(|_| ())
    })
}

fn unstar_message(ctx: Arc<MailUserContext>, id: LocalMessageId) -> Command<Messages> {
    Command::from_future(async move {
        MailMessage::action_unstar(ctx.action_queue(), vec![id])
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
    id: LocalMessageId,
    label_id: LocalLabelId,
) -> Command<Messages> {
    let current_label_id = mailbox.label_id();
    Command::from_future(async move {
        MailMessage::action_move(ctx.action_queue(), current_label_id, label_id, vec![id])
            .await
            .context("Failed to move message")
            .map(|_| ())
    })
}

fn block_sender(
    ctx: Arc<MailUserContext>,
    email: String,
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
