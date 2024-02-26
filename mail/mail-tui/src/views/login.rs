use crate::events::login::LoginEvents;
use crate::events::AppEvents;
use crate::state::LoginState;
use crate::view::View;
use crate::views::mailbox::ConversationView;
use crate::views::{AppViewContext, TotpView};
use crossterm::event::{Event, KeyCode};
use ratatui::layout::{Alignment, Constraint, Direction, Flex, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Masked, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;
use secrecy::SecretString;
use tui_input::backend::crossterm::EventHandler;

pub struct LoginView {
    logging_in: bool,
    input_index: usize,
    inputs: [tui_input::Input; 2],
}

const MAX_INPUT_INDICES: usize = 2;
const EMAIL_INDEX: usize = 0;
const PASSWORD_INDEX: usize = 1;
impl LoginView {
    pub fn new() -> Self {
        Self {
            logging_in: false,
            input_index: EMAIL_INDEX,
            inputs: [
                tui_input::Input::default().with_cursor(1),
                tui_input::Input::default().with_cursor(1),
            ],
        }
    }
}
impl View<AppViewContext, AppEvents> for LoginView {
    fn on_enter(&mut self, ctx: &mut AppViewContext) {
        self.logging_in = false;
        ctx.state_mut().logout();
    }

    fn on_exit(&mut self, _: &mut AppViewContext) {
        self.inputs[PASSWORD_INDEX].reset();
    }

    fn draw(&mut self, _: &AppViewContext, frame: &mut Frame, area: Rect) {
        if self.logging_in {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .flex(Flex::Center)
                .constraints([Constraint::Length(1)])
                .split(area);
            frame.render_widget(Text::from("Logging in...").centered(), chunks[0]);
        } else {
            let [_, email_area, password_area, _] = Layout::default()
                .direction(Direction::Vertical)
                .flex(Flex::Center)
                .constraints([
                    Constraint::Fill(1),
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Fill(1),
                ])
                .areas(area);

            let mut draw_text_box = |index: usize, label: &str, area: Rect| {
                let vertical_layout = Layout::default()
                    .direction(Direction::Horizontal)
                    .flex(Flex::Center)
                    .constraints([
                        Constraint::Percentage(10),
                        Constraint::Percentage(20),
                        Constraint::Percentage(10),
                    ])
                    .split(area);
                let horizontal = Layout::default()
                    .direction(Direction::Vertical)
                    .flex(Flex::Center)
                    .constraints([
                        Constraint::Length(1),
                        Constraint::Length(1),
                        Constraint::Length(1),
                    ])
                    .split(vertical_layout[0]);
                frame.render_widget(
                    Text::from(label.bold()).alignment(Alignment::Right),
                    horizontal[1],
                );

                let mut block = Block::default().borders(Borders::all());

                let input = &self.inputs[index];

                if self.input_index == index {
                    block = block.bold();
                }

                let p = if index == PASSWORD_INDEX {
                    Paragraph::new(Masked::new(input.value(), '*'))
                } else {
                    Paragraph::new(input.value())
                }
                .wrap(Wrap { trim: true })
                .block(block);
                frame.render_widget(p, vertical_layout[1]);
                if self.input_index == index {
                    let width = vertical_layout[1].width.max(3) - 3;
                    let scroll = input.visual_scroll(width as usize);
                    frame.set_cursor(
                        vertical_layout[1].x
                            + u16::try_from(input.cursor().max(scroll) - scroll)
                                .expect("invalid range")
                            + 1,
                        horizontal[1].y,
                    );
                }
            };

            draw_text_box(0, "Email:", email_area);
            draw_text_box(1, "Password:", password_area);
        }
    }

    fn draw_help(&self, _: &AppViewContext, frame: &mut Frame, area: Rect) {
        frame.render_widget(Text::from("(Enter) Submit"), area);
    }

    fn on_event(&mut self, ctx: &mut AppViewContext, event: AppEvents) -> Option<AppEvents> {
        match event {
            AppEvents::Login(e) => {
                match e {
                    LoginEvents::LoginFailed(err) => {
                        ctx.set_error("Failed t login", err);
                        self.logging_in = false;
                    }
                    LoginEvents::LoginSuccess(r) => match r {
                        Ok(r) => {
                            ctx.state_mut().login_state = LoginState::LoggedIn(r);
                            ctx.push_view(ConversationView::new())
                        }
                        Err(e) => {
                            ctx.set_error("Failed to prepare user data", e);
                        }
                    },
                    LoginEvents::LoginNeed2FA(t) => {
                        ctx.state_mut().login_state = LoginState::AwaitingTotp(t);
                        ctx.push_view(TotpView::new())
                    }
                    _ => {}
                }
                None
            }
            _ => Some(event),
        }
    }
    fn on_input(&mut self, ctx: &mut AppViewContext, event: &Event) {
        if let Event::Key(k) = event {
            match k.code {
                KeyCode::Enter => {
                    self.do_login(ctx);
                }
                KeyCode::Tab => {
                    self.input_index = (self.input_index + 1) % MAX_INPUT_INDICES;
                }
                _ => {
                    self.inputs[self.input_index].handle_event(event);
                }
            }
        }
    }

    fn name(&self) -> &'static str {
        "Login"
    }
}

impl LoginView {
    fn do_login(&mut self, ctx: &mut AppViewContext) {
        if self.logging_in {
            return;
        }
        self.logging_in = true;
        let dispatcher = ctx.dispatcher();
        let email = self.inputs[EMAIL_INDEX].value().to_string();
        let password = SecretString::from(self.inputs[PASSWORD_INDEX].to_string());
        ctx.state_mut().login(dispatcher, email, password);
    }
}
