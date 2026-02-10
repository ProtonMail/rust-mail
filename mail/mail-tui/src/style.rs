use ratatui::style::{Color, Style};

pub fn list_highlight_style() -> Style {
    Style {
        fg: Some(Color::Magenta),
        bg: Some(Color::White),
        underline_color: None,
        add_modifier: Default::default(),
        sub_modifier: Default::default(),
    }
}

pub fn background_style() -> Style {
    Style {
        bg: Some(Color::Magenta),
        fg: Some(Color::White),
        underline_color: None,
        add_modifier: Default::default(),
        sub_modifier: Default::default(),
    }
}
