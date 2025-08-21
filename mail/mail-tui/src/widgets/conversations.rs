use super::utils::{date_from_timestamp, format_flags};
use crate::widgets::utils::format_senders;
use crate::widgets::{AsIntoTable, IntoTable};
use proton_mail_common::datatypes::ContextualConversation;
use ratatui::layout::Constraint;
use ratatui::prelude::*;
use ratatui::widgets::{Cell, Row};

impl AsIntoTable for Vec<ContextualConversation> {
    fn as_table(&self) -> IntoTable<'_> {
        let rows = self.iter().map(|conv| {
            let flags = format_flags(conv.is_starred, false, conv.expiration_time);

            let date = if conv.display_snooze_reminder || conv.snoozed_until.is_some() {
                date_from_timestamp(conv.snooze_time).fg(Color::Yellow)
            } else {
                let date = date_from_timestamp(conv.time);
                Span::raw(date)
            };

            let num_attachments = conv.num_attachments;
            let num_labels = conv.custom_labels.len();
            let senders = format_senders(&conv.senders);

            let num_messages = if conv.total_messages == 0 {
                String::new()
            } else if conv.total_messages < 100 {
                format!("[ {:02}]", conv.total_messages)
            } else {
                "[99+]".to_owned()
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
                Cell::from(num_messages),
                Cell::from(flags),
                Cell::from(senders),
                Cell::from(conv.subject.clone()),
            ]);

            if conv.num_unread != 0 {
                row.bold()
            } else {
                row
            }
        });

        let widths = [
            Constraint::Length(16), // Date
            Constraint::Length(2),  // Labels
            Constraint::Length(2),  // Attachments
            Constraint::Length(5),  // Messages
            Constraint::Length(5),  // Flags
            Constraint::Fill(2),    // Sender
            Constraint::Fill(4),    // Subject
        ];

        let headers = Row::new([
            Cell::from("Date"),
            Cell::from("#L"),
            Cell::from("#A"),
            Cell::from("#M"),
            Cell::from("Flags"),
            Cell::from("Sender"),
            Cell::from("Subject"),
        ])
        .bold();

        IntoTable::new(rows, widths, headers)
    }
}
