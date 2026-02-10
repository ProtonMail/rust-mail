use crate::widgets::utils::{date_from_timestamp, format_recipients, sender_name};
use crate::widgets::{AsIntoTable, IntoTable};
use proton_mail_common::datatypes::MessageRecipientDisplayMode;
use proton_mail_common::models::Message;
use ratatui::layout::Constraint;
use ratatui::prelude::*;
use ratatui::widgets::{Cell, Row};

use super::utils::format_flags;

impl AsIntoTable for Vec<Message> {
    fn as_table(&self) -> IntoTable<'_> {
        message_as_table(self, MessageRecipientDisplayMode::Sender)
    }
}

pub fn message_as_table(
    messages: &[Message],
    recipient_display_mode: MessageRecipientDisplayMode,
) -> IntoTable<'_> {
    let rows = messages.iter().map(|msg| {
        let flags = format_flags(msg.is_starred(), msg.is_rsvp(), msg.expiration_time);
        let num_attachments = msg.num_attachments;
        let num_labels = msg.custom_labels.len();

        let date = if msg.display_snooze_reminder() || msg.snoozed_until().is_some() {
            date_from_timestamp(msg.snooze_time).fg(Color::Yellow)
        } else {
            let date = date_from_timestamp(msg.time);
            Span::raw(date)
        };

        let sender = match recipient_display_mode {
            MessageRecipientDisplayMode::Sender => sender_name(&msg.sender).to_owned(),
            MessageRecipientDisplayMode::Recipients => format_recipients(&msg.to_list),
        };

        let row = Row::new([
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
            Cell::from(flags),
            Cell::from(sender),
            Cell::from(msg.subject.as_str()),
        ]);

        if msg.unread { row.bold() } else { row }
    });

    let widths = [
        Constraint::Length(16), // Date
        Constraint::Length(2),  // Labels
        Constraint::Length(2),  // Attachments
        Constraint::Length(5),  // Flags
        Constraint::Fill(2),    // Sender
        Constraint::Fill(4),    // Subject
    ];

    let header = Row::new([
        Cell::from("Date"),
        Cell::from("#L"),
        Cell::from("#A"),
        Cell::from("Flags"),
        match recipient_display_mode {
            MessageRecipientDisplayMode::Sender => Cell::from("Sender"),
            MessageRecipientDisplayMode::Recipients => Cell::from("Recipients"),
        },
        Cell::from("Subject"),
    ])
    .bold();

    IntoTable::new(rows, widths, header)
}

// TODO:
//* Message body widget with scroll bar and paragraph for content (use text.content_length() for height)
//* conversation messages live query
//* Use conversation state code to mimic message rendering??
