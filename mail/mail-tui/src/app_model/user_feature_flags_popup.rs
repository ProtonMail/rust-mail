use crate::app::Command;
use crate::app_model::Popup;
use crate::messages::Messages;
use crate::widgets::utils::ScrollableState;
use crate::widgets::{ScrollableList, ScrollableListState, TextInput, TextInputState};
use proton_core_common::actions::user_feature_flags::OverrideFlag;
use proton_core_common::models::{ModelExtension, UserFeatureFlag};
use proton_mail_common::MailUserContext;
use ratatui::Frame;
use ratatui::crossterm::event::{Event, KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, Paragraph};
use std::sync::Arc;

struct FlagInfo {
    name: String,
    enabled: bool,
    writable: bool,
    source: String,
    overridden: bool,
}

pub struct UserFeatureFlagsPopup {
    ctx: Arc<MailUserContext>,
    flags: Vec<FlagInfo>,
    filtered_flags: Vec<usize>,
    list_state: ScrollableListState,
    filter_input_state: TextInputState,
}

impl UserFeatureFlagsPopup {
    fn new(ctx: Arc<MailUserContext>, flags: Vec<FlagInfo>) -> Self {
        let filtered_flags = (0..flags.len()).collect();
        Self {
            ctx,
            flags,
            filtered_flags,
            list_state: ScrollableListState::new(Some(0)),
            filter_input_state: TextInputState::new().selected(true),
        }
    }

    pub fn open(ctx: Arc<MailUserContext>) -> Command<Messages> {
        Command::task(async move {
            let tether = match ctx.user_stash().connection().await {
                Ok(t) => t,
                Err(e) => {
                    return Command::message(Messages::DisplayError(
                        Some("Feature Flags".to_owned()),
                        anyhow::anyhow!("Failed to connect to stash: {}", e),
                    ));
                }
            };

            let db_flags = match UserFeatureFlag::all(&tether).await {
                Ok(flags) => flags,
                Err(e) => {
                    return Command::message(Messages::DisplayError(
                        Some("Feature Flags".to_owned()),
                        anyhow::anyhow!("Failed to fetch flags: {}", e),
                    ));
                }
            };

            let flags: Vec<FlagInfo> = db_flags
                .into_iter()
                .map(|flag| FlagInfo {
                    name: flag.name.clone(),
                    enabled: flag.is_enabled(),
                    writable: flag.writable,
                    source: format!("{:?}", flag.source),
                    overridden: flag.overriden_to.is_some(),
                })
                .collect();

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
                .filter(|(_, flag)| flag.name.to_lowercase().contains(&filter_lower))
                .map(|(idx, _)| idx)
                .collect();
        }

        if !self.filtered_flags.is_empty() {
            self.list_state = ScrollableListState::new(Some(0));
        }
    }

    fn refresh(&self) -> Command<Messages> {
        let ctx = self.ctx.clone();
        Command::task(async move {
            if let Err(e) = ctx.user_context().feature_flags().refresh().await {
                return Command::message(Messages::DisplayError(
                    Some("Feature Flags".to_owned()),
                    anyhow::anyhow!("Failed to refresh: {}", e),
                ));
            }

            Self::open(ctx)
        })
    }

    fn toggle_selected(&mut self) -> Command<Messages> {
        let Some(selected_idx) = self.list_state.selected() else {
            return Command::None;
        };

        let Some(&flag_idx) = self.filtered_flags.get(selected_idx) else {
            return Command::None;
        };

        let flag = &self.flags[flag_idx];

        if !flag.writable {
            return Command::message(Messages::DisplayError(
                Some("Feature Flags".to_owned()),
                anyhow::anyhow!("Flag '{}' is not writable", flag.name),
            ));
        }

        let flag_name = flag.name.clone();
        let new_value = !flag.enabled;
        let ctx = self.ctx.clone();

        Command::batch([
            Command::message(Messages::DismissPopup),
            Command::task(async move {
                let action = OverrideFlag::new(flag_name.clone(), new_value);

                match ctx.user_context().queue().queue_action(action).await {
                    Ok(_) => Self::open(ctx),
                    Err(e) => Command::message(Messages::DisplayError(
                        Some("Feature Flags".to_owned()),
                        anyhow::anyhow!("Failed to toggle flag '{}': {}", flag_name, e),
                    )),
                }
            }),
        ])
    }
}

impl Popup for UserFeatureFlagsPopup {
    fn title(&self) -> Option<String> {
        Some("Feature Flags (User)".to_string())
    }

    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        let Event::Key(key) = event else {
            return Command::None;
        };

        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => return Command::message(Messages::DismissPopup),
            (KeyCode::Char('r'), m) if m.contains(KeyModifiers::CONTROL) => {
                return self.refresh();
            }
            (KeyCode::Char('t'), m) if m.contains(KeyModifiers::CONTROL) => {
                return self.toggle_selected();
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
            Constraint::Length(2),
        ])
        .areas(area);

        let items: Vec<ListItem> = self
            .filtered_flags
            .iter()
            .map(|&idx| {
                let flag = &self.flags[idx];
                let status = if flag.enabled { "✓" } else { "✗" };
                let writable_indicator = if flag.writable { "[W]" } else { "   " };
                let override_indicator = if flag.overridden { "*" } else { " " };

                let text = format!(
                    "{} {} {}{} ({})",
                    status, writable_indicator, flag.name, override_indicator, flag.source
                );

                let style = if flag.writable {
                    Style::default()
                } else {
                    Style::default().fg(Color::DarkGray)
                };

                ListItem::from(text).style(style)
            })
            .collect();

        frame.render_stateful_widget(
            TextInput::new("").with_max_label_length(0),
            filter_area,
            &mut self.filter_input_state,
        );

        let list = ScrollableList::new(List::new(items).block(Block::default()));
        frame.render_stateful_widget(list, list_area, &mut self.list_state);

        let help = vec![
            Line::from(vec![
                Span::raw("↑↓: Navigate | "),
                Span::raw("Ctrl+r: Refresh | "),
                Span::raw("Ctrl+t: Toggle | "),
                Span::raw("Esc: Close"),
            ]),
            Line::from(vec![
                Span::raw("[W]: Writable | "),
                Span::raw("*: Overridden"),
            ]),
        ];
        frame.render_widget(Paragraph::new(help), help_area);
    }

    fn height(&self) -> Constraint {
        Constraint::Percentage(70)
    }

    fn width(&self) -> Constraint {
        Constraint::Percentage(80)
    }
}
