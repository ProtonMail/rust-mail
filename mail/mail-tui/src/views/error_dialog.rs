use crate::tui_utils::inset_rect;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::Text;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use ratatui::Frame;
use std::error::Error;

pub struct ErrorDialog {
    text: String,
    source: String,
}

impl ErrorDialog {
    pub fn new(source: String, error: Box<dyn Error>) -> Self {
        let text = error.to_string();
        Self { source, text }
    }
    pub fn draw(&self, frame: &mut Frame) {
        let area = centered_rect(60, 60, frame.size());
        let block = Block::new()
            .white()
            .borders(Borders::ALL)
            .title(format!("Error: {}", self.source))
            .bold()
            .border_type(BorderType::Double);
        let [_, msg, _, instructions] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Min(3),
            Constraint::Fill(1),
            Constraint::Length(2),
        ])
        .flex(Flex::Center)
        .areas(inset_rect(area, 2));
        frame.render_widget(block.white().on_red(), area);
        frame.render_widget(
            Paragraph::new(self.text.as_str())
                .centered()
                .white()
                .bold()
                .wrap(Wrap { trim: false }),
            msg,
        );
        frame.render_widget(
            Text::from("Press Enter to continue...")
                .centered()
                .white()
                .bold(),
            instructions,
        );
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}
