use crate::app::Command;
use crate::app_model::Popup;
use crate::messages::Messages;
use crate::widgets::utils::ScrollableState;
use crate::widgets::{ScrollableList, ScrollableListState, TextInput, TextInputState};
use proton_mail_common::MailContext;
use ratatui::Frame;
use ratatui::crossterm::event::{Event, KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, Paragraph};
use std::sync::Arc;

pub struct GlobalFeatureFlagsPopup {
    ctx: Arc<MailContext>,
    flags: Vec<(String, bool)>,
    filtered_flags: Vec<usize>,
    list_state: ScrollableListState,
    filter_input_state: TextInputState,
}

impl GlobalFeatureFlagsPopup {
    fn new(ctx: Arc<MailContext>, flags: Vec<(String, bool)>) -> Self {
        let filtered_flags = (0..flags.len()).collect();
        Self {
            ctx,
            flags,
            filtered_flags,
            list_state: ScrollableListState::new(Some(0)),
            filter_input_state: TextInputState::new().selected(true),
        }
    }

    pub fn open(ctx: Arc<MailContext>) -> Command<Messages> {
        let ctx_clone = ctx.clone();
        Command::task(async move {
            let flags = ctx_clone.core_context().feature_flags().list_all().await;
            let popup = Self::new(ctx, flags);
            Command::message(Messages::RaisePopup(Box::new(popup)))
        })
    }

    fn update_filter(&mut self) {
        let filter_text = self.filter_input_state.value();
        if filter_text.is_empty() {
            self.filtered_flags = (0..self.flags.len()).collect();
        } else {
            let filter_lower = filter_text.to_lowercase();
            self.filtered_flags = self
                .flags
                .iter()
                .enumerate()
                .filter(|(_, (name, _))| name.to_lowercase().contains(&filter_lower))
                .map(|(idx, _)| idx)
                .collect();
        }

        if !self.filtered_flags.is_empty() {
            self.list_state = ScrollableListState::new(Some(0));
        }
    }

    fn refresh(ctx: Arc<MailContext>) -> Command<Messages> {
        Command::batch([
            Command::message(Messages::DismissPopup),
            Command::task(async move {
                if let Err(e) = ctx.core_context().feature_flags().refresh().await {
                    return Command::message(Messages::DisplayError(
                        Some("Feature Flags".to_owned()),
                        anyhow::anyhow!("Failed to refresh: {}", e),
                    ));
                }

                Self::open(ctx)
            }),
        ])
    }
}

impl Popup for GlobalFeatureFlagsPopup {
    fn title(&self) -> Option<String> {
        Some("Feature Flags (Global)".to_string())
    }

    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        let Event::Key(key) = event else {
            return Command::None;
        };

        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => return Command::message(Messages::DismissPopup),
            (KeyCode::Char('r'), m) if m.contains(KeyModifiers::CONTROL) => {
                return Self::refresh(self.ctx.clone());
            }
            (KeyCode::Up | KeyCode::Down, _) => {
                self.list_state.handle_event(key.code);
                return Command::None;
            }
            _ => {}
        }

        self.filter_input_state.handle_event(&event);
        self.update_filter();
        Command::None
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let [filter_area, list_area, help_area] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(area);

        frame.render_stateful_widget(
            TextInput::new("").with_max_label_length(0),
            filter_area,
            &mut self.filter_input_state,
        );

        let items: Vec<ListItem> = self
            .filtered_flags
            .iter()
            .map(|&idx| {
                let (name, enabled) = &self.flags[idx];
                let status = if *enabled { "✓" } else { "✗" };
                ListItem::from(format!("{status} {name}"))
            })
            .collect();

        let list = ScrollableList::new(List::new(items).block(Block::default()));
        frame.render_stateful_widget(list, list_area, &mut self.list_state);

        let help = Line::from(vec![
            Span::raw("↑↓: Navigate | "),
            Span::raw("Ctrl+r: Refresh | "),
            Span::raw("Esc: Close"),
        ]);
        frame.render_widget(Paragraph::new(help), help_area);
    }

    fn height(&self) -> Constraint {
        Constraint::Percentage(70)
    }

    fn width(&self) -> Constraint {
        Constraint::Percentage(60)
    }
}
