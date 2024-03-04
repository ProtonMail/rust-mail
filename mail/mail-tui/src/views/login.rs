use crate::events::login::LoginEvent;
use crate::events::AppEvent;
use crate::state::LoginState;
use crate::view::View;
use crate::views::AppViewContext;
use crate::widgets::{HelpCategory, HelpItem, TextInput, TextInputState};
use crossterm::event::{Event, KeyCode};
use ratatui::layout::{Constraint, Direction, Flex, Layout, Margin, Rect};
use ratatui::text::Text;
use ratatui::Frame;
use secrecy::SecretString;

pub struct LoginView {
    input_index: usize,
    email_input_state: TextInputState,
    password_input_state: TextInputState,
}

const MAX_INPUT_INDICES: usize = 2;
const EMAIL_INDEX: usize = 0;
const PASSWORD_INDEX: usize = 1;
impl LoginView {
    pub fn new() -> Self {
        Self {
            input_index: EMAIL_INDEX,
            email_input_state: TextInputState::new().selected(true),
            password_input_state: TextInputState::new().secret(true),
        }
    }
}
impl View<AppViewContext, AppEvent> for LoginView {
    fn on_enter(&mut self, ctx: &mut AppViewContext) {
        ctx.app_local_dispatcher().queue_event(LoginEvent::Logout);
    }

    fn on_exit(&mut self, _: &mut AppViewContext) {
        self.password_input_state.reset();
        self.email_input_state.reset();
    }

    fn draw(&mut self, ctx: &AppViewContext, frame: &mut Frame, area: Rect) {
        if matches!(ctx.state().login_state, LoginState::LoggingIn) {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .flex(Flex::Center)
                .constraints([Constraint::Length(1)])
                .split(area);
            frame.render_widget(Text::from("Logging in...").centered(), chunks[0]);
        } else {
            let area = area.inner(&Margin {
                horizontal: 10,
                vertical: 2,
            });
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

            const MAX_LABEL_SIZE: u16 = 10;
            frame.render_stateful_widget(
                TextInput::new("Email:").with_max_label_length(MAX_LABEL_SIZE),
                email_area,
                &mut self.email_input_state,
            );

            frame.render_stateful_widget(
                TextInput::new("Password:").with_max_label_length(MAX_LABEL_SIZE),
                password_area,
                &mut self.password_input_state,
            );

            let (x, y) = self.active_text_input_state_mut().frame_cursor();
            frame.set_cursor(x, y);
        }
    }

    fn help_items(&self) -> &[HelpCategory] {
        static ITEMS: [HelpCategory; 1] = [HelpCategory {
            name: "Login",
            items: &[
                HelpItem {
                    key: "Esc",
                    description: "Return to Sessions",
                },
                HelpItem {
                    key: "Enter",
                    description: "Login",
                },
            ],
        }];

        &ITEMS
    }

    fn on_input(&mut self, ctx: &mut AppViewContext, event: &Event) {
        if let Event::Key(k) = event {
            match k.code {
                KeyCode::Esc => {
                    ctx.app_local_dispatcher().pop_view();
                }
                KeyCode::Enter => {
                    self.do_login(ctx);
                }
                KeyCode::Tab => {
                    self.active_text_input_state_mut().set_selected(false);
                    self.input_index = (self.input_index + 1) % MAX_INPUT_INDICES;
                    self.active_text_input_state_mut().set_selected(true);
                }
                _ => {
                    self.active_text_input_state_mut().handle_event(event);
                }
            }
        }
    }

    fn name(&self) -> &'static str {
        "Login"
    }
}

impl LoginView {
    fn active_text_input_state_mut(&mut self) -> &mut TextInputState {
        match self.input_index {
            EMAIL_INDEX => &mut self.email_input_state,
            PASSWORD_INDEX => &mut self.password_input_state,
            _ => unreachable!(),
        }
    }
    fn do_login(&mut self, ctx: &mut AppViewContext) {
        if !matches!(ctx.state().login_state, LoginState::LoggedOut) {
            return;
        }
        ctx.app_local_dispatcher()
            .queue_event(LoginEvent::LoginRequest {
                user: self.email_input_state.value().to_string(),
                password: SecretString::new(self.password_input_state.value().to_string()),
            });
    }
}
