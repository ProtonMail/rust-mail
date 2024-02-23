use ratatui::layout::Constraint;
use ratatui::layout::{Layout, Rect};

pub fn inset_rect(area: Rect, quantity: u16) -> Rect {
    inset_rect_vertical(inset_rect_horizontal(area, quantity), quantity)
}

pub fn inset_rect_vertical(area: Rect, quantity: u16) -> Rect {
    let [_, v, _] = Layout::vertical([
        Constraint::Length(quantity),
        Constraint::Min(1),
        Constraint::Length(quantity),
    ])
    .areas(area);
    v
}

pub fn inset_rect_horizontal(area: Rect, quantity: u16) -> Rect {
    let [_, h, _] = Layout::horizontal([
        Constraint::Length(quantity),
        Constraint::Min(1),
        Constraint::Length(quantity),
    ])
    .areas(area);
    h
}
