#![allow(unused, clippy::module_name_repetitions)]
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Flex, Layout, Rect};
use ratatui::prelude::{Masked, StatefulWidget, Stylize, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Widget, Wrap};
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

pub struct TextInputState {
    selected: bool,
    secret: bool,
    input: Input,
    frame_cursor: (u16, u16),
}

impl TextInputState {
    pub fn new() -> Self {
        Self {
            secret: false,
            selected: false,
            input: Input::new(String::new()),
            frame_cursor: (0, 0),
        }
    }

    pub fn with_value(v: impl Into<String>) -> Self {
        Self {
            secret: false,
            selected: false,
            input: Input::new(v.into()),
            frame_cursor: (0, 0),
        }
    }
    pub fn is_selected(&self) -> bool {
        self.selected
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }

    pub fn is_secret(&self) -> bool {
        self.secret
    }

    pub fn secret(mut self, secret: bool) -> Self {
        self.secret = secret;
        self
    }

    pub fn set_secret(&mut self, secret: bool) {
        self.secret = secret;
    }

    pub fn value(&self) -> &str {
        self.input.value()
    }

    pub fn cursor(&self) -> usize {
        self.input.cursor()
    }

    pub fn reset(&mut self) {
        self.input.reset();
    }

    pub fn handle_event(
        &mut self,
        event: &crossterm::event::Event,
    ) -> Option<tui_input::StateChanged> {
        self.input.handle_event(event)
    }

    /// Get the cursor location to be displayed in the frame. Must be retrieved after drawing.
    pub fn frame_cursor(&self) -> (u16, u16) {
        self.frame_cursor
    }

    fn set_frame_cursor(&mut self, width: u16, height: u16) {
        self.frame_cursor = (width, height);
    }
}

pub struct TextInput<'a> {
    label: Text<'a>,
    max_label_width: Option<u16>,
}

impl<'a> TextInput<'a> {
    pub fn new(label: impl Into<Text<'a>>) -> Self {
        Self {
            label: label.into(),
            max_label_width: None,
        }
    }

    pub fn with_max_label_length(mut self, width: u16) -> Self {
        self.max_label_width = Some(width);
        self
    }
}

impl StatefulWidget for TextInput<'_> {
    type State = TextInputState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let label_constraint = if let Some(max) = self.max_label_width {
            Constraint::Max(max)
        } else {
            Constraint::Min(16)
        };
        let [input_area] = Layout::vertical([Constraint::Length(3)]).areas(area);
        let [label_area, text_area] = Layout::default()
            .direction(Direction::Horizontal)
            .flex(Flex::Center)
            .constraints([label_constraint, Constraint::Fill(20)])
            .areas(input_area);
        let [_, label_area, _] = Layout::default()
            .direction(Direction::Vertical)
            .flex(Flex::Center)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .areas(label_area);

        // Draw label
        self.label
            .alignment(Alignment::Right)
            .render(label_area, buf);
        let block = {
            let mut block = Block::default().borders(Borders::all());
            if state.selected {
                block = block.bold();
            }
            block
        };

        let paragraph = if state.is_secret() {
            Paragraph::new(Masked::new(state.input.value(), '*'))
        } else {
            Paragraph::new(state.input.value())
        }
        .wrap(Wrap { trim: true })
        .block(block);

        paragraph.render(text_area, buf);

        if state.selected {
            let width = text_area.width.max(3) - 3;
            let scroll = state.input.visual_scroll(width as usize);
            state.set_frame_cursor(
                text_area.x
                    + u16::try_from(state.cursor().max(scroll) - scroll).expect("invalid range")
                    + 1,
                label_area.y,
            );
        }
    }
}
