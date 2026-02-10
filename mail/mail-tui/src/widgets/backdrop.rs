use ratatui::prelude::{Buffer, Rect};
use ratatui::style::Color;
use ratatui::widgets::Widget;

#[derive(Clone, Copy, Debug)]
pub struct Backdrop;

impl Widget for Backdrop {
    fn render(self, area: Rect, buf: &mut Buffer) {
        for x in area.left()..area.right() {
            for y in area.top()..area.bottom() {
                buf[(x, y)].set_fg(Color::DarkGray).set_bg(Color::Black);
            }
        }
    }
}
