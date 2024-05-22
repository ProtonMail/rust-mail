use crate::widgets::utils::{date_from_timestamp, sender_name};
use crate::widgets::AsTable;
use proton_mail_common::db::LocalMessageMetadata;
use ratatui::layout::Constraint;
use ratatui::prelude::*;
use ratatui::widgets::{Cell, Row, Table};

impl AsTable for Vec<LocalMessageMetadata> {
    fn as_table(&self) -> Table<'_> {
        let rows = self.iter().map(|msg| {
            let starred = if msg.starred { "★" } else { " " };
            let date = date_from_timestamp(msg.time);
            let num_attachments = msg.attachments.as_ref().map_or(0, Vec::len);
            let num_labels = msg.labels.as_ref().map_or(0, Vec::len);
            let sender = sender_name(&msg.sender);

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
            Cell::from("#L"),
            Cell::from("#A"),
            Cell::from(""),
            Cell::from("Sender"),
            Cell::from("Subject"),
        ])
        .bold();

        Table::new(rows, widths)
            .column_spacing(1)
            .header(headers)
            .highlight_style(Style::new().reversed())
    }
}
