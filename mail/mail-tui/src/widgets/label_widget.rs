use crate::widgets::widget_list::ListableWidget;
use proton_mail_common::proton_mail_db::LocalLabelWithCount;
use ratatui::buffer::Buffer;
use ratatui::layout::{Layout, Rect};
use ratatui::prelude::Constraint;
use ratatui::style::Stylize;
use ratatui::text::Text;
use ratatui::widgets::Widget;

pub struct LabelWidget<'a> {
    label: &'a LocalLabelWithCount,
}

impl<'a> LabelWidget<'a> {
    pub fn new(label: &'a LocalLabelWithCount) -> Self {
        Self { label }
    }
}

impl<'a> ListableWidget for LabelWidget<'a> {
    fn height(&self) -> u16 {
        1
    }

    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let [text_area, _, unread_area, _] = Layout::horizontal([
            Constraint::Min(15),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(1),
        ])
        .areas(area);

        let label_name = self
            .label
            .path
            .as_deref()
            .unwrap_or(self.label.name.as_str());

        Text::from(label_name).render(text_area, buf);
        if self.label.unread_count != 0 {
            Text::from(format!("{:02}", self.label.unread_count))
                .bold()
                .render(unread_area, buf);
        }
    }
}
