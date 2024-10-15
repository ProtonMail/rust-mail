#![allow(clippy::module_name_repetitions)]

use crate::app::Command;
use crate::app_model::mailbox::model::StateHandler;
use crate::app_model::mailbox::{BackgroundSender, Item, Message, MessageMessage};
use crate::app_model::watcher::WatchHandle;
use crate::messages::Messages;
use crate::widgets::utils::{date_from_timestamp, format_sender, format_senders};
use crate::widgets::{
    AsTable, CenteredThrobber, ScrollableParagraph, ScrollableParagraphState, ScrollableTable,
    ScrollableTableState,
};
use anyhow::anyhow;
use crossterm::event::{Event, KeyCode, KeyModifiers};
use futures::FutureExt;
use proton_core_common::datatypes::LocalId;
use proton_mail_common::datatypes::{ContextualConversation, MimeType};
use proton_mail_common::decrypted_message::DecryptedMessageBody;
use proton_mail_common::models::{MailSettings, Message as MailMessage};
use proton_mail_common::{AppError, MailContext, Mailbox, MailboxResult};
use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;
use std::sync::Arc;
use throbber_widgets_tui::ThrobberState;

/// Displays a list of messages based of message metadata. If a conversation is opened the message
/// body will be displayed.
pub struct MessagesState {
    _query: WatchHandle,
    messages: Vec<MailMessage>,
    table_state: ScrollableTableState,
    open_message: DecryptedMessageStatus,
}

const MESSAGE_DISPLAY_SIZE: u16 = 100;
const MIN_LIST_DISPLAY_SIZE: u16 = 20;
impl MessagesState {
    pub async fn new(mbox: &Mailbox, sender: BackgroundSender) -> MailboxResult<Self> {
        let (messages, receiver) =
            MailMessage::watch_in_label(mbox.label_id(), mbox.user_context().user_stash()).await?;

        let ctx_cloned = mbox.user_context();
        let label_id = mbox.label_id();
        let watcher = WatchHandle::new_dampened(
            receiver,
            move || {
                let ctx_cloned = Arc::clone(&ctx_cloned);
                async move {
                    Some(
                        match MailMessage::in_label(label_id, ctx_cloned.user_stash(), None).await {
                            Ok(messages) => MessageMessage::Refreshed(messages).into(),
                            Err(e) => {
                                let e = anyhow!("Message list Query error: {e}");
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
            messages,
            table_state: ScrollableTableState::new(Some(0)),
            open_message: DecryptedMessageStatus::None,
        })
    }

    pub async fn from_conversation(
        mbox: &Mailbox,
        conversation_id: LocalId,
        sender: BackgroundSender,
    ) -> MailboxResult<Self> {
        let Some(conv_and_messages) = ContextualConversation::conversation_and_messages(
            conversation_id,
            mbox.label_id(),
            mbox.user_context().user_stash(),
            mbox.user_context().api(),
        )
        .await?
        else {
            return Err(AppError::ConversationNotFound(conversation_id).into());
        };

        let receiver = ContextualConversation::watch_conversation_and_messages(
            conversation_id,
            mbox.user_context().user_stash(),
        )
        .await?;

        let context_cloned = mbox.user_context();
        let watcher = WatchHandle::new_dampened(
            receiver,
            move || {
                let context_cloned = Arc::clone(&context_cloned);
                async move {
                    Some(
                        match MailMessage::in_conversation(
                            conversation_id,
                            context_cloned.user_stash(),
                            None,
                        )
                        .await
                        {
                            Ok(m) => MessageMessage::Refreshed(m).into(),
                            Err(e) => {
                                let e = anyhow!("Message list Query error: {e}");
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

        let index = conv_and_messages
            .messages
            .iter()
            .position(|m| m.local_id.unwrap() == conv_and_messages.message_id_to_open)
            .unwrap_or(0);

        Ok(Self {
            _query: watcher,
            messages: conv_and_messages.messages,
            table_state: ScrollableTableState::new(Some(index)),
            open_message: DecryptedMessageStatus::None,
        })
    }

    pub fn open_message_body(&mut self, _: &MailContext, mbox: &Mailbox) -> Command<Messages> {
        let Some(metadata) = self.selected_message() else {
            tracing::warn!("No message selected");
            return Command::None;
        };

        let mbox = mbox.clone();
        self.open_message = DecryptedMessageStatus::Loading(ThrobberState::default());

        Command::task(async move {
            let decrypted =
                match MailMessage::message_body(&mbox.user_context(), metadata.local_id.unwrap())
                    .await
                {
                    Ok(m) => m,
                    Err(e) => {
                        let e = anyhow!("Failed to get message body {e}");
                        tracing::error!("{e}");
                        return Command::message(e.into());
                    }
                };

            let result = process_message(&decrypted)
                .map(|m| Box::new(DecryptedMessage::new(metadata, decrypted, m)));

            Command::message(MessageMessage::OpenMessageBodyResult(result).into())
        })
    }

    fn display_message(&mut self, message: anyhow::Result<Box<DecryptedMessage>>) {
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

    fn selected_message_id(&self) -> Option<LocalId> {
        let index = self.table_state.selected()?;
        self.messages.get(index).map(|c| c.local_id.unwrap())
    }
}

impl StateHandler for MessagesState {
    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        let Event::Key(key) = event else {
            return Command::None;
        };

        if matches!(
            self.open_message,
            DecryptedMessageStatus::Success(_) | DecryptedMessageStatus::Error(_)
        ) && key.code == KeyCode::Esc
        {
            return Command::message(MessageMessage::CloseMessageBody.into());
        }

        if let DecryptedMessageStatus::Success(state) = &mut self.open_message {
            match key.code {
                KeyCode::Up => {
                    if key.modifiers.intersects(KeyModifiers::SHIFT) {
                        state.scroll_state.scroll_up();
                        return Command::None;
                    }
                }
                KeyCode::Down => {
                    if key.modifiers.intersects(KeyModifiers::SHIFT) {
                        state.scroll_state.scroll_down();
                        return Command::None;
                    }
                }
                _ => {}
            };
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
            KeyCode::Char('u') => self
                .selected_message_id()
                .map(|id| Command::message(MessageMessage::MarkMessageUnread(id).into()))
                .unwrap_or_default(),
            KeyCode::Char('r') => self
                .selected_message_id()
                .map(|id| Command::message(MessageMessage::MarkMessageRead(id).into()))
                .unwrap_or_default(),
            KeyCode::Char('d') => self
                .selected_message_id()
                .map(|id| Command::message(MessageMessage::DeleteMessage(id).into()))
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
            KeyCode::Char('L') => self
                .selected_message_id()
                .map(|id| Command::message(Message::OpenUnlabelItemPopup(Item::Message(id)).into()))
                .unwrap_or_default(),
            KeyCode::Enter => self
                .selected_message_id()
                .map(|_| Command::message(MessageMessage::OpenMessageBody.into()))
                .unwrap_or_default(),
            _ => Command::None,
        }
    }

    async fn update(
        &mut self,
        ctx: &MailContext,
        message: Message,
        mbox: &Mailbox,
        _: &Arc<MailSettings>,
        _: &BackgroundSender,
    ) -> Command<Messages> {
        let Message::MessageState(message) = message else {
            return Command::None;
        };

        match message {
            MessageMessage::OpenMessageBody => {
                return self.open_message_body(ctx, mbox);
            }
            MessageMessage::OpenMessageBodyResult(r) => {
                self.display_message(r);
            }
            MessageMessage::CloseMessageBody => {
                self.close_message();
            }
            MessageMessage::Refreshed(messages) => self.messages_refreshed(messages),
            MessageMessage::DeleteMessage(id) => {
                return delete_message(mbox, id).await;
            }
            MessageMessage::MoveMessage(msg_id, id) => {
                return move_message(mbox, msg_id, id).await;
            }
            MessageMessage::LabelMessage(msg_id, id) => {
                return label_message(mbox, msg_id, id).await;
            }
            MessageMessage::UnlabelMessage(msg_id, id) => {
                return unlabel_message(mbox, msg_id, id).await;
            }
            MessageMessage::MarkMessageRead(id) => {
                return mark_message_read(mbox, id).await;
            }
            MessageMessage::MarkMessageUnread(id) => {
                return mark_message_unread(mbox, id).await;
            }
        }
        Command::None
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let table_area = self.open_message.draw(frame, area);

        if let Some(table_area) = table_area {
            let table = self.messages.as_table();
            let scrollable_table = ScrollableTable::new(table, self.messages.len());

            frame.render_stateful_widget(scrollable_table, table_area, &mut self.table_state);
        }
    }
}

pub struct DecryptedMessage {
    metadata: MailMessage,
    decrypted_body: DecryptedMessageBody,
    content: Option<String>,
    scroll_state: ScrollableParagraphState,
    num_lines: usize,
    date: String,
    sender: String,
    to_list: String,
    cc_list: String,
    bcc_list: String,
    label_list: String,
}

/// # Safety
///
/// The `NodeRef` type is not send by default, but the data is not shared outside of the crate
/// so it is safe.
unsafe impl Send for DecryptedMessage {}

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
    pub fn new(
        metadata: MailMessage,
        decrypted_body: DecryptedMessageBody,
        content: Option<String>,
    ) -> Self {
        let text = content.as_deref().unwrap_or(&decrypted_body.body);
        let num_lines = text.chars().filter(|c| *c == '\n').count();

        let date = date_from_timestamp(metadata.time);
        let sender = format_sender(&metadata.sender);
        let to_list = format_senders(&metadata.to_list);
        let cc_list = format_senders(&metadata.cc_list);
        let bcc_list = format_senders(&metadata.bcc_list);
        let label_list = metadata
            .custom_labels
            .iter()
            .map(|l| l.name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        Self {
            metadata,
            decrypted_body,
            content,
            scroll_state: ScrollableParagraphState::new(),
            num_lines,
            date,
            sender,
            to_list,
            cc_list,
            bcc_list,
            label_list,
        }
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect) {
        let rows = vec![
            Row::new([
                Cell::from("Subject:"),
                Cell::from(self.metadata.subject.as_str()),
            ])
            .bold(),
            Row::new([Cell::from("Date:").bold(), Cell::from(self.date.as_str())]),
            Row::new([Cell::from("From:").bold(), Cell::from(self.sender.as_str())]),
            Row::new([Cell::from("To:").bold(), Cell::from(self.to_list.as_str())]),
            Row::new([Cell::from("CC:").bold(), Cell::from(self.cc_list.as_str())]),
            Row::new([
                Cell::from("BCC:").bold(),
                Cell::from(self.bcc_list.as_str()),
            ]),
            Row::new([
                Cell::from("Labels:").bold(),
                Cell::from(self.label_list.as_str()),
            ]),
        ];

        let [header_area, box_area, message_area] = Layout::vertical([
            Constraint::Length(u16::try_from(rows.len()).unwrap_or(7)),
            Constraint::Length(1),
            Constraint::Percentage(100),
        ])
        .areas(area);

        let widths = [Constraint::Length(10), Constraint::Fill(1)];
        let table = Table::new(rows, widths).column_spacing(1);

        frame.render_widget(table, header_area);

        frame.render_widget(Block::new().borders(Borders::TOP), box_area);
        let text = self.content.as_deref().unwrap_or(&self.decrypted_body.body);
        let paragraph = ScrollableParagraph::new(Paragraph::new(text), self.num_lines);
        frame.render_stateful_widget(paragraph, message_area, &mut self.scroll_state);
    }
}

pub(super) fn process_message(message: &DecryptedMessageBody) -> anyhow::Result<Option<String>> {
    match message.metadata.mime_type {
        MimeType::TextPlain => Ok(None),
        MimeType::TextHtml => html_to_text(&message.body).map(Some),
        _ => Err(anyhow!(
            "Unsupported mime type: {:?}",
            message.metadata.mime_type
        )),
    }
}

fn html_to_text(message: &str) -> anyhow::Result<String> {
    let cursor = std::io::Cursor::new(message);
    let config = html2text::config::plain();
    config
        .string_from_read(cursor, 80)
        .map_err(|e| anyhow!("Failed to parse HTML: {e}"))
}
async fn mark_message_read(mailbox: &Mailbox, id: LocalId) -> Command<Messages> {
    let ctx = mailbox.user_context();
    match MailMessage::action_mark_read(ctx.session(), ctx.queue(), mailbox.label_id(), vec![id])
        .await
    {
        Ok(_) => Command::None,
        Err(e) => {
            let e = anyhow!("Failed to mark message as read: {e}");
            tracing::error!("{e}");
            Command::message(e.into())
        }
    }
}

async fn mark_message_unread(mailbox: &Mailbox, id: LocalId) -> Command<Messages> {
    let ctx = mailbox.user_context();
    match MailMessage::action_mark_unread(ctx.session(), ctx.queue(), mailbox.label_id(), vec![id])
        .await
    {
        Ok(_) => Command::None,
        Err(e) => {
            let e = anyhow!("Failed to mark message as unread: {e}");
            tracing::error!("{e}");
            Command::message(e.into())
        }
    }
}

async fn delete_message(mailbox: &Mailbox, id: LocalId) -> Command<Messages> {
    let ctx = mailbox.user_context();
    match MailMessage::action_delete(ctx.session(), ctx.queue(), mailbox.label_id(), vec![id]).await
    {
        Ok(_) => Command::None,
        Err(e) => {
            let e = anyhow!("Failed to delete message: {e}");
            tracing::error!("{e}");
            Command::message(e.into())
        }
    }
}
async fn label_message(mailbox: &Mailbox, id: LocalId, label_id: LocalId) -> Command<Messages> {
    let ctx = mailbox.user_context();
    match MailMessage::action_apply_label(ctx.session(), ctx.queue(), label_id, vec![id]).await {
        Ok(_) => Command::None,
        Err(e) => {
            let e = anyhow!("Failed to apply label to message: {e}");
            tracing::error!("{e}");
            Command::message(e.into())
        }
    }
}

async fn unlabel_message(mailbox: &Mailbox, id: LocalId, label_id: LocalId) -> Command<Messages> {
    let ctx = mailbox.user_context();
    match MailMessage::action_remove_label(ctx.session(), ctx.queue(), label_id, vec![id]).await {
        Ok(_) => Command::None,
        Err(e) => {
            let e = anyhow!("Failed to apply label to message: {e}");
            tracing::error!("{e}");
            Command::message(e.into())
        }
    }
}
async fn move_message(mailbox: &Mailbox, id: LocalId, label_id: LocalId) -> Command<Messages> {
    let ctx = mailbox.user_context();
    match MailMessage::action_move(
        ctx.session(),
        ctx.queue(),
        mailbox.label_id(),
        label_id,
        vec![id],
    )
    .await
    {
        Ok(_) => Command::None,
        Err(e) => {
            let e = anyhow!("Failed to apply label to message: {e}");
            tracing::error!("{e}");
            Command::message(e.into())
        }
    }
}
