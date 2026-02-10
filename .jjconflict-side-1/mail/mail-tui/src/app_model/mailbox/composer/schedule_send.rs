use crate::app::Command;
use crate::app_model::Popup;
use crate::app_model::mailbox::ComposerMessage;
use crate::messages::Messages;
use crate::widgets::utils::parse_date_time;
use crate::widgets::{TextInput, TextInputState};
use chrono::Local;
use crossterm::event::{Event, KeyCode, KeyEvent};
use proton_mail_common::draft::ScheduleSendOptions;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::prelude::Margin;
use ratatui::style::{Style, Stylize};
use ratatui::text::Text;
use ratatui::widgets::{Block, HighlightSpacing, List, ListState};

pub struct ScheduleSendPopup {
    mode: Mode,
}

impl ScheduleSendPopup {
    pub fn new(options: ScheduleSendOptions<Local>) -> Self {
        Self {
            mode: Mode::Default(DefaultOptions::new(options)),
        }
    }
}

impl Popup for ScheduleSendPopup {
    fn title(&self) -> Option<String> {
        Some("Schedule Send Options".to_string())
    }

    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        let (cmd, swap_state) = match &mut self.mode {
            Mode::Default(m) => m.handle_event(&event),
            Mode::Custom(m) => (m.handle_event(&event), false),
        };

        if swap_state {
            self.mode = Mode::Custom(CustomOptions::new());
        }

        cmd
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        match &mut self.mode {
            Mode::Default(m) => {
                m.view(frame, area);
            }
            Mode::Custom(m) => {
                m.view(frame, area);
            }
        }
    }
}

enum Mode {
    Default(DefaultOptions),
    Custom(CustomOptions),
}

struct DefaultOptions {
    options: ScheduleSendOptions<Local>,
    list_state: ListState,
}

const CUSTOM_OPTION_INDEX: usize = 2;
const SEND_TOMORROW_OPTION_INDEX: usize = 0;
const SEND_NEXT_MONDAY_OPTION_INDEX: usize = 1;

impl DefaultOptions {
    fn new(options: ScheduleSendOptions<Local>) -> Self {
        Self {
            options,
            list_state: ListState::default().with_selected(Some(0)),
        }
    }

    fn handle_event(&mut self, event: &Event) -> (Command<Messages>, bool) {
        let Event::Key(key) = event else {
            return (Command::none(), false);
        };

        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.list_state.select_previous();
                (Command::none(), false)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.list_state.select_next();
                (Command::none(), false)
            }
            KeyCode::Enter => {
                if let Some(selected) = self.list_state.selected() {
                    match selected {
                        SEND_TOMORROW_OPTION_INDEX => (
                            Command::batch([
                                Command::message(Messages::DismissPopup),
                                Command::message(ComposerMessage::ScheduleSend(
                                    self.options.time_tomorrow,
                                )),
                            ]),
                            false,
                        ),
                        SEND_NEXT_MONDAY_OPTION_INDEX => (
                            Command::batch([
                                Command::message(Messages::DismissPopup),
                                Command::message(ComposerMessage::ScheduleSend(
                                    self.options.time_next_monday,
                                )),
                            ]),
                            false,
                        ),
                        CUSTOM_OPTION_INDEX => (Command::none(), true),
                        _ => (Command::none(), false),
                    }
                } else {
                    (Command::none(), false)
                }
            }
            _ => (Command::none(), false),
        }
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let tomorrow_text = format!(
            "Tomorrow ({})",
            self.options.time_tomorrow.format("%d/%m/%Y %H:%M")
        );
        let next_monday_text = format!(
            "Monday ({})",
            self.options.time_next_monday.format("%d/%m/%Y %H:%M")
        );
        let custom_text = format!(
            "Custom (Available={})",
            self.options.is_custom_datetime_available
        );

        let area = area.inner(Margin {
            vertical: 1,
            horizontal: 2,
        });
        let list = List::new([
            Text::from(tomorrow_text).centered(),
            Text::from(next_monday_text).centered(),
            Text::from(custom_text).centered(),
        ])
        .highlight_spacing(HighlightSpacing::Never)
        .highlight_style(Style::new().reversed())
        .block(Block::bordered());
        frame.render_stateful_widget(list, area, &mut self.list_state);
    }
}
struct CustomOptions {
    text_input_state: TextInputState,
    error: Option<String>,
}

impl CustomOptions {
    fn new() -> Self {
        Self {
            text_input_state: TextInputState::new().selected(true),
            error: None,
        }
    }

    fn handle_event(&mut self, event: &Event) -> Command<Messages> {
        self.text_input_state.handle_event(event);
        if let Event::Key(KeyEvent { code, .. }) = event
            && matches!(code, KeyCode::Enter)
        {
            match parse_date_time(self.text_input_state.value()) {
                Ok(date_time) => {
                    return Command::batch([
                        Command::message(Messages::DismissPopup),
                        Command::message(ComposerMessage::ScheduleSend(date_time)),
                    ]);
                }
                Err(e) => {
                    self.error = Some(format!("Parse Error: {e}"));
                }
            }
        }

        Command::none()
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let area = area.inner(Margin {
            vertical: 1,
            horizontal: 2,
        });

        let area = if let Some(error) = &self.error {
            let [_, error_area, _, remaining] = Layout::vertical([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(4),
            ])
            .areas(area);
            frame.render_widget(
                Text::from(error.as_str()).centered().red().bold(),
                error_area,
            );
            remaining
        } else {
            area
        };

        let [help_area, _, text_input_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(4),
        ])
        .areas(area);

        frame.render_widget(
            Text::from("Input Custom Date Time (format = DD/MM/YYYY HH:MM)").centered(),
            help_area,
        );
        frame.render_stateful_widget(
            TextInput::new("Custom Date"),
            text_input_area,
            &mut self.text_input_state,
        );
    }
}
