use crate::widgets::{utils, AsTable};
use proton_mail_common::db::LocalConversation;
use ratatui::layout::Constraint;
use ratatui::prelude::*;
use ratatui::widgets::{Cell, Row, Table};

impl AsTable for Vec<LocalConversation> {
    fn as_table(&self) -> Table<'_> {
        let rows = self.iter().map(|conv| {
            let starred = if conv.starred { "★" } else { " " };
            let date = utils::date_from_timestamp(conv.time);
            let num_attachments = conv.attachments.as_ref().map_or(0, Vec::len);
            let num_labels = conv.labels.as_ref().map_or(0, Vec::len);
            let senders = {
                if conv.senders.len() == 1 {
                    utils::sender_name(&conv.senders[0]).to_string()
                } else {
                    conv.senders
                        .iter()
                        .map(|s| utils::sender_name(s).to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                }
            };

            let num_messages = if conv.num_messages == 0 {
                String::new()
            } else if conv.num_messages < 100 {
                format!("[ {:02}]", conv.num_messages)
            } else {
                "[99+]".to_owned()
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
                Cell::from(num_messages),
                Cell::from(starred).bold(),
                Cell::from(senders),
                Cell::from(conv.subject.clone()),
            ]);
            if conv.num_unread != 0 {
                row = row.bold();
            }
            row
        });

        let widths = [
            Constraint::Length(16),
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(5),
            Constraint::Length(1),
            Constraint::Fill(2),
            Constraint::Fill(4),
        ];

        let headers = Row::new([
            Cell::from("Date"),
            Cell::from("#L"),
            Cell::from("#A"),
            Cell::from("#M"),
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
