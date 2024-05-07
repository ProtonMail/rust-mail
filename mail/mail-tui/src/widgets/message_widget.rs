use crate::widgets::widget_list::ListableWidget;
use chrono::DateTime;
use proton_mail_common::db::LocalMessageMetadata;
use proton_mail_common::proton_api_mail::domain::MessageAddress;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::prelude::Text;
use ratatui::style::Stylize;
use ratatui::widgets::Widget;

pub struct MessageWidget<'c> {
    msg: &'c LocalMessageMetadata,
}

impl<'c> MessageWidget<'c> {
    pub fn new(msg: &'c LocalMessageMetadata) -> Self {
        Self { msg }
    }
}

impl ListableWidget for MessageWidget<'_> {
    fn height(&self) -> u16 {
        3
    }
}

impl Widget for MessageWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let [sender_area, conv_area, label_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(area);
        // Sender
        let [sender_area, _, date_area] = Layout::horizontal([
            Constraint::Min(30),
            Constraint::Fill(1),
            Constraint::Length(20),
        ])
        .areas(sender_area);
        fn sender_name(s: &MessageAddress) -> &str {
            if s.name.is_empty() {
                s.address.as_str()
            } else {
                s.name.as_str()
            }
        }

        let senders = sender_name(&self.msg.sender);
        let line = Text::from(vec![senders.into(), self.msg.subject.clone().into()]);
        line.render(sender_area, buf);

        let date =
            DateTime::<chrono::Utc>::from_timestamp(i64::try_from(self.msg.time).unwrap(), 0)
                .unwrap();
        let date = DateTime::<chrono::Local>::from(date);
        let date_str = date.format("%d/%m/%Y %H:%M");
        Text::from(date_str.to_string())
            .right_aligned()
            .render(date_area, buf);
        // Title
        let [conv_area, _, icon_area] = Layout::horizontal([
            Constraint::Min(30),
            Constraint::Fill(1),
            Constraint::Length(6),
        ])
        .areas(conv_area);
        Text::from(self.msg.subject.as_str()).render(conv_area, buf);
        let mut icons = String::new();
        if self.msg.num_attachments != 0 {
            icons.push_str("[A]");
        }

        if self.msg.starred {
            icons.push_str(" ★ ");
        }

        if !icons.is_empty() {
            Text::from(icons.as_str())
                .right_aligned()
                .render(icon_area, buf);
        }

        // Labels
        if let Some(labels) = &self.msg.labels {
            Text::from(
                labels
                    .iter()
                    .map(|l| l.name.clone())
                    .collect::<Vec<_>>()
                    .join(" | "),
            )
            .italic()
            .render(label_area, buf)
        }
    }
}
