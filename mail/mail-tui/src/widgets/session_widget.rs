use crate::widgets::ListableWidget;
use proton_core_common::db::EncryptedUserSession;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::{Text, Widget};
use ratatui::style::Stylize;

pub struct SessionWidget<'a> {
    session: &'a EncryptedUserSession,
}

impl<'a> SessionWidget<'a> {
    pub fn new(session: &'a EncryptedUserSession) -> Self {
        Self { session }
    }
}

impl ListableWidget for SessionWidget<'_> {
    fn height(&self) -> u16 {
        2
    }
}

impl Widget for SessionWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let name = self.session.name.as_ref().unwrap_or(&self.session.email);
        Text::from(vec![
            name.clone().bold().into(),
            self.session.email.clone().into(),
        ])
        .render(area, buf)
    }
}
