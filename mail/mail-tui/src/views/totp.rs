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

pub struct TotpView {
    input_state: TextInputState,
}
impl TotpView {
    pub fn new() -> Self {
        Self {
            input_state: TextInputState::new().selected(true),
        }
    }
}
impl View<AppViewContext, AppEvent> for TotpView {
    fn on_exit(&mut self, _: &mut AppViewContext) {
        self.input_state.reset();
    }

    fn draw(&mut self, ctx: &AppViewContext, frame: &mut Frame, area: Rect) {
        if matches!(ctx.state().login_state, LoginState::SubmittingTotp) {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .flex(Flex::Center)
                .constraints([Constraint::Length(1)])
                .split(area);
            frame.render_widget(Text::from("Submitting Totp...").centered(), chunks[0]);
        } else {
            let area = area.inner(&Margin {
                horizontal: 10,
                vertical: 2,
            });
            let [_, totp_area, _] = Layout::default()
                .direction(Direction::Vertical)
                .flex(Flex::Center)
                .constraints([
                    Constraint::Fill(1),
                    Constraint::Length(3),
                    Constraint::Fill(1),
                ])
                .areas(area);

            frame.render_stateful_widget(
                TextInput::new("Two Factor Code:"),
                totp_area,
                &mut self.input_state,
            );

            let (x, y) = self.input_state.frame_cursor();
            frame.set_cursor(x, y);
        }
    }

    fn help_items(&self) -> &[HelpCategory] {
        static ITEMS: [HelpCategory; 1] = [HelpCategory {
            name: "2FA",
            items: &[
                HelpItem {
                    key: "Enter",
                    description: "Submit Code",
                },
                HelpItem {
                    key: "Esc",
                    description: "Return to Login",
                },
            ],
        }];

        &ITEMS
    }

    fn on_input(&mut self, ctx: &mut AppViewContext, event: &Event) {
        if let Event::Key(k) = event {
            match k.code {
                KeyCode::Enter => {
                    self.do_submit(ctx);
                }
                KeyCode::Esc => ctx.app_local_dispatcher().pop_view(),
                _ => {
                    self.input_state.handle_event(event);
                }
            }
        }
    }

    fn name(&self) -> &'static str {
        "2FA"
    }
}

impl TotpView {
    fn do_submit(&mut self, ctx: &mut AppViewContext) {
        let code = self.input_state.value().to_string();
        ctx.app_local_dispatcher()
            .queue_event(LoginEvent::TwoFARequest(code))
    }
}
