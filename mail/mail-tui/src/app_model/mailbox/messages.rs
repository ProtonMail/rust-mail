#![allow(clippy::module_name_repetitions)]

use crate::app::Command;
use crate::app_model::mailbox::composer::Composer;
use crate::app_model::mailbox::model::StateHandler;
use crate::app_model::mailbox::paginator::Paginator;
use crate::app_model::mailbox::{ConversationMessage, Item, Message, MessageMessage, ITEM_LIMIT};
use crate::app_model::watcher::WatchHandle;
use crate::app_model::YesNoPopup;
use crate::messages::Messages;
use crate::widgets::utils::{date_from_timestamp, format_recipients, format_sender};
use crate::widgets::{
    AsTable, CenteredThrobber, ScrollableParagraph, ScrollableParagraphState, ScrollableTable,
    ScrollableTableState,
};
use anyhow::{anyhow, Context};
use futures::FutureExt;
use proton_core_common::datatypes::{LocalId, LocalLabelId};
use proton_mail_common::datatypes::ContextualConversation;
use proton_mail_common::decrypted_message::{DecryptedMessageBody, TransformOpts};
use proton_mail_common::draft::ReplyMode;
use proton_mail_common::models::{
    Label, MailSettings, Message as MailMessage, MessageDataSource, PaginatorFilter,
    PaginatorSearchOptions,
};
use proton_mail_common::{AppError, MailContext, MailUserContext, Mailbox, MailboxResult};
use ratatui::crossterm::event::{Event, KeyCode, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;
use stash::stash::WatcherHandle;
use std::sync::Arc;
use throbber_widgets_tui::ThrobberState;

use super::LabelAs;

/// Displays a list of messages based of message metadata. If a conversation is opened the message
/// body will be displayed.
pub struct MessagesState {
    messages: Vec<MailMessage>,
    table_state: ScrollableTableState,
    open_message: DecryptedMessageStatus,
    mode: Mode,
}

#[allow(dead_code)] // Watcher handle is needed to keep state
enum Mode {
    Label(Paginator<MailMessage, MessageDataSource>),
    Conversation(WatchHandle),
}

const MESSAGE_DISPLAY_SIZE: u16 = 100;
const MIN_LIST_DISPLAY_SIZE: u16 = 20;
impl MessagesState {
    pub(super) fn build(mbox: Mailbox, label: Label) -> Command<Messages> {
        let ctx = mbox.user_context();
        let label_id = mbox.label_id();
        Command::task(async move {
            match Self::new_impl(ctx, label_id).await {
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
    ) -> MailboxResult<(Self, Command<Messages>)> {
        let (paginator, command) = Paginator::new(
            || {
                async move {
                    Ok(MailMessage::paginate_in_label(
                        &ctx,
                        label_id,
                        ITEM_LIMIT.try_into().unwrap(),
                        PaginatorFilter::default(),
                        PaginatorSearchOptions::default(),
                        true,
                    )
                    .await?)
                }
                .boxed()
            },
            |result| match result {
                Ok(messages) => MessageMessage::Refreshed(messages).into(),
                Err(e) => {
                    let e = anyhow!("Message Reload Query error: {e}");
                    tracing::error!("{e}");
                    e.into()
                }
            },
        )
        .await?;

        let messages = paginator.next_page().await?;

        Ok((
            Self {
                messages,
                table_state: ScrollableTableState::new(Some(0)),
                open_message: DecryptedMessageStatus::None,
                mode: Mode::Label(paginator),
            },
            command,
        ))
    }

    pub(super) fn from_conversation(mbox: &Mailbox, conversation_id: LocalId) -> Command<Messages> {
        let ctx = mbox.user_context();
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
                    tracing::error!("{e}");
                    Command::message(ConversationMessage::OpenConversationFailed(e).into())
                }
            }
        })
    }
    async fn from_conversation_impl(
        ctx: Arc<MailUserContext>,
        label_id: LocalLabelId,
        conversation_id: LocalId,
    ) -> MailboxResult<(Self, Command<Messages>)> {
        let Some(conv_and_messages) = ContextualConversation::conversation_and_messages(
            conversation_id,
            label_id,
            ctx.user_stash(),
            ctx.api(),
        )
        .await?
        else {
            return Err(AppError::ConversationNotFound(conversation_id).into());
        };

        let WatcherHandle {
            handle, receiver, ..
        } = ContextualConversation::watch(ctx.user_stash())?;
        let (watcher, background_command) =
            WatchHandle::new_dampened(receiver, handle, move || {
                let tether = ctx.user_stash().connection();
                async move {
                    Some(
                        match MailMessage::in_conversation(conversation_id, &tether).await {
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
            },
            background_command,
        ))
    }

    pub fn open_message_body(&mut self, mbox: &Mailbox) -> Command<Messages> {
        let Some(metadata) = self.selected_message() else {
            tracing::warn!("No message selected");
            return Command::None;
        };

        let mbox = mbox.clone();
        self.open_message = DecryptedMessageStatus::Loading(ThrobberState::default());

        Command::task(async {
            #[allow(clippy::redundant_closure_call)] // Poor's man try blocks
            let c: anyhow::Result<_> = (|| async move {
                let decrypted =
                    MailMessage::message_body(mbox.user_context(), metadata.local_id.unwrap())
                        .await
                        .context("Failed to get message body")?;
                let html = decrypted
                    .transformed(&mbox.user_context(), TransformOpts::default())
                    .await;
                let html = html_to_text(&html.body)?;
                Ok(Box::new(DecryptedMessage::new(
                    metadata,
                    decrypted,
                    Some(html),
                )))
            })()
            .await;

            Command::message(MessageMessage::OpenMessageBodyResult(c).into())
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
    #[allow(clippy::too_many_lines)]
    fn handle_event(&mut self, mbox: &Mailbox, event: Event) -> Command<Messages> {
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
                KeyCode::Char('k') | KeyCode::Up => {
                    if key.modifiers.intersects(KeyModifiers::SHIFT) {
                        state.scroll_state.scroll_up();
                        return Command::None;
                    }
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    if key.modifiers.intersects(KeyModifiers::SHIFT) {
                        state.scroll_state.scroll_down();
                        return Command::None;
                    }
                }
                _ => {}
            };
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
                Command::None
            }
            KeyCode::Char('e') => {
                let context = mbox.user_context();
                self.selected_message_id()
                    .map(|id| Composer::open(context, id))
                    .unwrap_or_default()
            }
            KeyCode::Char('u') => self
                .selected_message_id()
                .map(|id| Command::message(MessageMessage::MarkMessageUnread(id).into()))
                .unwrap_or_default(),
            KeyCode::Char('r') => self
                .selected_message_id()
                .map(|id| {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        if key.modifiers.contains(KeyModifiers::SHIFT) {
                            Composer::reply(mbox.user_context(), id, ReplyMode::All)
                        } else {
                            Composer::reply(mbox.user_context(), id, ReplyMode::Sender)
                        }
                    } else {
                        Command::message(MessageMessage::MarkMessageRead(id).into())
                    }
                })
                .unwrap_or_default(),
            KeyCode::Char('f') => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.selected_message_id()
                        .map(|id| Composer::reply(mbox.user_context(), id, ReplyMode::Forward))
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
                        .map(|id| Composer::reply(mbox.user_context(), id, ReplyMode::All))
                        .unwrap_or_default()
                } else {
                    Command::None
                }
            }
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
            KeyCode::Enter => self
                .selected_message_id()
                .map(|_| Command::message(MessageMessage::OpenMessageBody.into()))
                .unwrap_or_default(),
            _ => Command::None,
        }
    }

    fn update(
        &mut self,
        _: &MailContext,
        message: Message,
        mbox: &Mailbox,
        _: &Arc<MailSettings>,
    ) -> Command<Messages> {
        let Message::MessageState(message) = message else {
            return Command::None;
        };

        match message {
            MessageMessage::OpenMessageBody => {
                return self.open_message_body(mbox);
            }
            MessageMessage::OpenMessageBodyResult(r) => {
                self.display_message(r);
            }
            MessageMessage::CloseMessageBody => {
                self.close_message();
            }
            MessageMessage::Refreshed(messages) => self.messages_refreshed(messages),
            MessageMessage::DeleteMessage(id) => {
                return delete_message(mbox, id);
            }
            MessageMessage::MoveMessage(msg_id, id) => {
                return move_message(mbox, msg_id, id);
            }
            MessageMessage::LabelMessage(label_as) => {
                return label_message(mbox, *label_as);
            }
            MessageMessage::MarkMessageRead(id) => {
                return mark_message_read(mbox, id);
            }
            MessageMessage::MarkMessageUnread(id) => {
                return mark_message_unread(mbox, id);
            }
            MessageMessage::StarMessage(id) => {
                return star_message(mbox, id);
            }
            MessageMessage::UnstarMessage(id) => {
                return unstar_message(mbox, id);
            }
            MessageMessage::NextPage(messages) => self.messages.extend(messages),
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
        let to_list = format_recipients(&metadata.to_list);
        let cc_list = format_recipients(&metadata.cc_list);
        let bcc_list = format_recipients(&metadata.bcc_list);
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

fn html_to_text(message: &str) -> anyhow::Result<String> {
    // TODO: Best effort terminal image rendering. See https://docs.rs/termimage/latest/termimage/
    let cursor = std::io::Cursor::new(message);
    let config = html2text::config::plain();
    config
        .string_from_read(cursor, 80)
        .map_err(|e| anyhow!("Failed to parse HTML: {e}"))
}
fn mark_message_read(mailbox: &Mailbox, id: LocalId) -> Command<Messages> {
    let ctx = mailbox.user_context();
    let current_label_id = mailbox.label_id();
    Command::task(async move {
        match ctx
            .with_queue(|queue| MailMessage::action_mark_read(queue, current_label_id, vec![id]))
            .await
        {
            Ok(_) => Command::None,
            Err(e) => {
                let e = anyhow!("Failed to mark message as read: {e}");
                tracing::error!("{e}");
                Command::message(e.into())
            }
        }
    })
}

fn mark_message_unread(mailbox: &Mailbox, id: LocalId) -> Command<Messages> {
    let ctx = mailbox.user_context();
    let current_label_id = mailbox.label_id();
    Command::task(async move {
        match ctx
            .with_queue(|queue| MailMessage::action_mark_unread(queue, current_label_id, vec![id]))
            .await
        {
            Ok(_) => Command::None,
            Err(e) => {
                let e = anyhow!("Failed to mark message as unread: {e}");
                tracing::error!("{e}");
                Command::message(e.into())
            }
        }
    })
}

fn delete_message(mailbox: &Mailbox, id: LocalId) -> Command<Messages> {
    let ctx = mailbox.user_context();
    let current_label_id = mailbox.label_id();
    Command::message(Messages::raise_popup(
        YesNoPopup::new(
            "Confirm Message Delete",
            "Are you sure you wish to permanently delete the currently selected message?",
        )
        .on_accept(Command::task(async move {
            match ctx
                .with_queue(|queue| MailMessage::action_delete(queue, current_label_id, vec![id]))
                .await
            {
                Ok(_) => Command::None,
                Err(e) => {
                    let e = anyhow!("Failed to delete message: {e}");
                    tracing::error!("{e}");
                    Command::message(e.into())
                }
            }
        })),
    ))
}

fn star_message(mailbox: &Mailbox, id: LocalId) -> Command<Messages> {
    let ctx = mailbox.user_context();
    Command::task(async move {
        match ctx
            .with_queue(|queue| MailMessage::action_star(queue, vec![id]))
            .await
        {
            Ok(_) => Command::None,
            Err(e) => {
                let e = anyhow!("Failed to apply label to message: {e}");
                tracing::error!("{e}");
                Command::message(e.into())
            }
        }
    })
}

fn unstar_message(mailbox: &Mailbox, id: LocalId) -> Command<Messages> {
    let ctx = mailbox.user_context();
    Command::task(async move {
        match ctx
            .with_queue(|queue| MailMessage::action_unstar(queue, vec![id]))
            .await
        {
            Ok(_) => Command::None,
            Err(e) => {
                let e = anyhow!("Failed to apply label to message: {e}");
                tracing::error!("{e}");
                Command::message(e.into())
            }
        }
    })
}
fn label_message(
    mailbox: &Mailbox,
    LabelAs {
        source_label_id,
        item_ids: conversation_ids,
        selected_label_ids,
        partially_selected_label_ids,
        must_archive,
    }: LabelAs,
) -> Command<Messages> {
    let ctx = mailbox.user_context();
    Command::task(async move {
        match ctx
            .with_queue(|queue| {
                MailMessage::action_label_as(
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
                let e = anyhow!("Failed to apply label to message: {e}");
                tracing::error!("{e}");
                Command::message(e.into())
            }
        }
    })
}

fn move_message(mailbox: &Mailbox, id: LocalId, label_id: LocalLabelId) -> Command<Messages> {
    let ctx = mailbox.user_context();
    let current_label_id = mailbox.label_id();
    Command::task(async move {
        match ctx
            .with_queue(|queue| {
                MailMessage::action_move(queue, current_label_id, label_id, vec![id])
            })
            .await
        {
            Ok(_) => Command::None,
            Err(e) => {
                let e = anyhow!("Failed to apply label to message: {e}");
                tracing::error!("{e}");
                Command::message(e.into())
            }
        }
    })
}
