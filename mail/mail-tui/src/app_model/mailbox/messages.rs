#![allow(clippy::module_name_repetitions)]

use crate::app_model::mailbox::model::StateHandler;
use crate::app_model::mailbox::{LiveQueryBuilder, Message, MessageMessage, ITEM_LIMIT};
use crate::app_model::BackgroundSender;
use crate::messages::Messages;
use crate::widgets::utils::{date_from_timestamp, format_sender, format_senders};
use crate::widgets::{
    AsTable, CenteredThrobber, ScrollableParagraph, ScrollableParagraphState, ScrollableTable,
    ScrollableTableState,
};
use anyhow::anyhow;
use crossterm::event::{Event, KeyCode, KeyModifiers};
use proton_core_common::db::proton_sqlite3::Observed;
use proton_core_common::db::DBResult;
use proton_mail_common::db::{LocalConversationId, LocalMessageId, LocalMessageMetadata};
use proton_mail_common::exports::tracing;
use proton_mail_common::proton_api_mail::domain::MimeType;
use proton_mail_common::{DecryptedMessageBody, MailContext, Mailbox, MailboxResult};
use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;
use throbber_widgets_tui::ThrobberState;

/// Displays a list of messages based of message metadata. If a conversation is opened the message
/// body will be displayed.
pub struct MessagesState {
    _query: Observed,
    messages: Vec<LocalMessageMetadata>,
    table_state: ScrollableTableState,
    open_message: DecryptedMessageStatus,
}

const MESSAGE_DISPLAY_SIZE: u16 = 100;
const MIN_LIST_DISPLAY_SIZE: u16 = 20;
impl MessagesState {
    pub fn new(mbox: &Mailbox, background_sender: BackgroundSender) -> MailboxResult<Self> {
        let messages = mbox.messages(ITEM_LIMIT)?;
        let query = mbox.new_messages_query(
            LiveQueryBuilder::new(messages_refreshed_converter, background_sender),
            ITEM_LIMIT,
        )?;

        Ok(Self {
            _query: query,
            messages,
            table_state: ScrollableTableState::new(Some(0)),
            open_message: DecryptedMessageStatus::None,
        })
    }

    pub async fn from_conversation(
        mbox: &Mailbox,
        conversation_id: LocalConversationId,
        background_sender: BackgroundSender,
    ) -> MailboxResult<Self> {
        let (to_select_id, messages) = mbox.conversation_messages(conversation_id).await?;
        let query = mbox
            .new_conversation_message_query(
                LiveQueryBuilder::new(messages_refreshed_converter, background_sender.clone()),
                conversation_id,
            )
            .await?;

        let index = messages
            .iter()
            .position(|m| m.id == to_select_id)
            .unwrap_or(0);

        Ok(Self {
            _query: query,
            messages,
            table_state: ScrollableTableState::new(Some(index)),
            open_message: DecryptedMessageStatus::None,
        })
    }

    pub fn open_message_body(
        &mut self,
        ctx: &MailContext,
        mbox: &Mailbox,
        sender: &BackgroundSender,
    ) {
        let Some(metadata) = self.selected_message() else {
            tracing::warn!("No message selected");
            return;
        };

        let mbox = mbox.clone();
        let sender = sender.clone();

        ctx.async_runtime().spawn(async move {
            let decrypted = match mbox.message_body(metadata.id).await {
                Ok(m) => m,
                Err(e) => {
                    let e = anyhow!("Failed to get message body {e}");
                    tracing::error!("{e}");
                    sender.send(e.into());
                    return;
                }
            };

            let result = process_message(&decrypted)
                .map(|m| Box::new(DecryptedMessage::new(metadata, decrypted, m)));

            sender.send(MessageMessage::OpenMessageBodyResult(result).into());
        });

        self.open_message = DecryptedMessageStatus::Loading(ThrobberState::default());
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

    fn messages_refreshed(&mut self, messages: Vec<LocalMessageMetadata>) {
        self.messages = messages;
    }

    fn selected_message(&self) -> Option<LocalMessageMetadata> {
        let index = self.table_state.selected()?;
        self.messages.get(index).cloned()
    }

    fn selected_message_id(&self) -> Option<LocalMessageId> {
        let index = self.table_state.selected()?;
        self.messages.get(index).map(|c| c.id)
    }
}

impl StateHandler for MessagesState {
    fn handle_event(&mut self, event: Event) -> Option<Messages> {
        let Event::Key(key) = event else {
            return None;
        };

        if matches!(
            self.open_message,
            DecryptedMessageStatus::Success(_) | DecryptedMessageStatus::Error(_)
        ) && key.code == KeyCode::Esc
        {
            return Some(MessageMessage::CloseMessageBody.into());
        }

        if let DecryptedMessageStatus::Success(state) = &mut self.open_message {
            match key.code {
                KeyCode::Up => {
                    if key.modifiers.intersects(KeyModifiers::SHIFT) {
                        state.scroll_state.scroll_up();
                        return None;
                    }
                }
                KeyCode::Down => {
                    if key.modifiers.intersects(KeyModifiers::SHIFT) {
                        state.scroll_state.scroll_down();
                        return None;
                    }
                }
                _ => {}
            };
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
            KeyCode::Enter => self
                .selected_message_id()
                .map(|_| MessageMessage::OpenMessageBody.into()),
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
        let Message::MessageState(message) = message else {
            return None;
        };

        match message {
            MessageMessage::OpenMessageBody => {
                self.open_message_body(ctx, mbox, sender);
            }
            MessageMessage::OpenMessageBodyResult(r) => {
                self.display_message(r);
            }
            MessageMessage::CloseMessageBody => {
                self.close_message();
            }
            MessageMessage::Refreshed(messages) => self.messages_refreshed(messages),
        }
        None
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
    metadata: LocalMessageMetadata,
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
        metadata: LocalMessageMetadata,
        decrypted_body: DecryptedMessageBody,
        content: Option<String>,
    ) -> Self {
        let text = content.as_deref().unwrap_or(decrypted_body.body());
        let num_lines = text.chars().filter(|c| *c == '\n').count();

        let date = date_from_timestamp(metadata.time);
        let sender = format_sender(&metadata.sender);
        let to_list = format_senders(&metadata.to);
        let cc_list = format_senders(&metadata.cc);
        let bcc_list = format_senders(&metadata.bcc);
        let label_list = metadata.labels.as_ref().map_or(String::new(), |l| {
            l.iter()
                .map(|l| l.name.clone())
                .collect::<Vec<_>>()
                .join(", ")
        });
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
        let text = self
            .content
            .as_deref()
            .unwrap_or(self.decrypted_body.body());
        let paragraph = ScrollableParagraph::new(Paragraph::new(text), self.num_lines);
        frame.render_stateful_widget(paragraph, message_area, &mut self.scroll_state);
    }
}

pub(super) fn process_message(message: &DecryptedMessageBody) -> anyhow::Result<Option<String>> {
    match message.metadata().mime_type {
        MimeType::TextPlain => Ok(None),
        MimeType::TextHTML => html_to_text(message.body()).map(Some),
        _ => Err(anyhow!(
            "Unsupported mime type: {:?}",
            message.metadata().mime_type
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

fn messages_refreshed_converter(messages: DBResult<Vec<LocalMessageMetadata>>) -> Messages {
    match messages {
        Ok(m) => MessageMessage::Refreshed(m).into(),
        Err(e) => {
            let e = anyhow!("Message list Query error: {e}");
            tracing::error!("{e}");
            e.into()
        }
    }
}
