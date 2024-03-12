use crate::widgets::widget_list::ListableWidget;
use chrono::DateTime;
use proton_mail_common::proton_api_mail::domain::MessageAddress;
use proton_mail_common::proton_mail_db::LocalConversationWithContext;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::prelude::Text;
use ratatui::style::Stylize;
use ratatui::widgets::Widget;

pub struct ConversationWidget<'c> {
    conv: &'c LocalConversationWithContext,
}

impl<'c> ConversationWidget<'c> {
    pub fn new(conv: &'c LocalConversationWithContext) -> Self {
        Self { conv }
    }
}

impl ListableWidget for ConversationWidget<'_> {
    fn height(&self) -> u16 {
        3
    }
}

impl Widget for ConversationWidget<'_> {
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

        let senders = {
            if self.conv.senders.len() == 1 {
                sender_name(&self.conv.senders[0]).to_string()
            } else {
                self.conv
                    .senders
                    .iter()
                    .map(|s| sender_name(s).to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            }
        };
        let line = Text::from(vec![
            if self.conv.num_messages > 1 {
                format!("[{}] {}", self.conv.num_messages, senders).into()
            } else {
                senders.into()
            },
            self.conv.subject.clone().into(),
        ]);
        line.render(sender_area, buf);

        let date = DateTime::<chrono::Utc>::from_timestamp(
            i64::try_from(self.conv.context_time).unwrap(),
            0,
        )
        .unwrap();
        let date = DateTime::<chrono::Local>::from(date);
        let date_str = date.format("%d/%m/%Y %H:%M");
        Text::from(date_str.to_string())
            .right_aligned()
            .render(date_area, buf);
        // Title
        let [conv_area, _, icon_area] =
            Layout::horizontal([Constraint::Min(30), Constraint::Fill(1), Constraint::Min(6)])
                .areas(conv_area);
        Text::from(self.conv.subject.as_str()).render(conv_area, buf);
        if self.conv.context_num_attachments != 0 {
            Text::from("[A]").right_aligned().render(icon_area, buf);
        }

        // Labels
        if let Some(labels) = &self.conv.labels {
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
