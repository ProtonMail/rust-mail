use crate::widgets::AsTable;
use crate::widgets::utils::{date_from_timestamp, format_recipients, sender_name};
use proton_mail_common::datatypes::MessageRecipientDisplayMode;
use proton_mail_common::models::Message;
use ratatui::layout::Constraint;
use ratatui::prelude::*;
use ratatui::widgets::{Cell, Row, Table};

impl AsTable for Vec<Message> {
    fn as_table(&self) -> Table<'_> {
        message_as_table(self, MessageRecipientDisplayMode::Sender)
    }
}

pub fn message_as_table(
    messages: &[Message],
    recipient_display_mode: MessageRecipientDisplayMode,
) -> Table<'_> {
    let rows = messages.iter().map(|msg| {
        let starred = if msg.is_starred() { "★" } else { " " };
        let date = date_from_timestamp(msg.time);
        let num_attachments = msg.num_attachments;
        let num_labels = msg.custom_labels.len();
        let sender = match recipient_display_mode {
            MessageRecipientDisplayMode::Sender => sender_name(&msg.sender).to_owned(),
            MessageRecipientDisplayMode::Recipients => format_recipients(&msg.to_list),
        };

        let mut row = Row::new([
            Cell::from(date),
            Cell::from(if num_labels != 0 {
                format!("{num_labels:02}")
            } else {
                String::new()
            }),
            Cell::from(if num_attachments != 0 {
                format!("{num_attachments:02}")
            } else {
                String::new()
            }),
            Cell::from(starred).bold(),
            Cell::from(sender),
            Cell::from(msg.subject.clone()),
        ]);
        if msg.unread {
            row = row.bold();
        }
        row
    });

    let widths = [
        Constraint::Length(16),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Fill(2),
        Constraint::Fill(4),
    ];

    let headers = Row::new([
        Cell::from("Date"),
        Cell::from("#L"), // Labels
        Cell::from("#A"), // Attachments
        Cell::from(""),   // Starred
        match recipient_display_mode {
            MessageRecipientDisplayMode::Sender => Cell::from("Sender"),
            MessageRecipientDisplayMode::Recipients => Cell::from("Recipients"),
        },
        Cell::from("Subject"),
    ])
    .bold();

    Table::new(rows, widths)
        .column_spacing(1)
        .header(headers)
        .highlight_style(Style::new().reversed())
}

// TODO:
//* Message body widget with scroll bar and paragraph for content (use text.content_length() for height)
//* conversation messages live query
//* Use conversation state code to mimic message rendering??
