use crate::events::login::LoginEvents;
use crate::events::AppEvents;
use crate::state::LoginState;
use crate::view::View;
use crate::views::mailbox::ConversationView;
use crate::views::AppViewContext;
use crossterm::event::{Event, KeyCode};
use ratatui::layout::{Alignment, Constraint, Direction, Flex, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::Text;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;
use tui_input::backend::crossterm::EventHandler;

pub struct TotpView {
    submitting: bool,
    input: tui_input::Input,
}
impl TotpView {
    pub fn new() -> Self {
        Self {
            submitting: false,
            input: tui_input::Input::default().with_cursor(1),
        }
    }
}
impl View<AppViewContext, AppEvents> for TotpView {
    fn on_enter(&mut self, _: &mut AppViewContext) {
        self.submitting = false;
    }

    fn on_exit(&mut self, _: &mut AppViewContext) {
        self.input.reset();
    }

    fn draw(&mut self, _: &AppViewContext, frame: &mut Frame, area: Rect) {
        if self.submitting {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .flex(Flex::Center)
                .constraints([Constraint::Length(1)])
                .split(area);
            frame.render_widget(Text::from("Submitting Totp...").centered(), chunks[0]);
        } else {
            let [_, totp_area, _] = Layout::default()
                .direction(Direction::Vertical)
                .flex(Flex::Center)
                .constraints([
                    Constraint::Fill(1),
                    Constraint::Length(3),
                    Constraint::Fill(1),
                ])
                .areas(area);

            let vertical_layout = Layout::default()
                .direction(Direction::Horizontal)
                .flex(Flex::Center)
                .constraints([
                    Constraint::Percentage(10),
                    Constraint::Percentage(20),
                    Constraint::Percentage(10),
                ])
                .split(totp_area);
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
                Text::from("TOTP".bold()).alignment(Alignment::Right),
                horizontal[1],
            );

            let block = Block::default().borders(Borders::all()).bold();
            let p = Paragraph::new(self.input.value())
                .wrap(Wrap { trim: true })
                .block(block);
            frame.render_widget(p, vertical_layout[1]);
            let width = vertical_layout[1].width.max(3) - 3;
            let scroll = self.input.visual_scroll(width as usize);
            frame.set_cursor(
                vertical_layout[1].x
                    + u16::try_from(self.input.cursor().max(scroll) - scroll)
                        .expect("invalid range")
                    + 1,
                horizontal[1].y,
            );
        }
    }

    fn draw_help(&self, _: &AppViewContext, frame: &mut Frame, area: Rect) {
        frame.render_widget(Text::from("(Esc) Abort|(Enter) Submit"), area);
    }

    fn on_event(&mut self, ctx: &mut AppViewContext, event: AppEvents) -> Option<AppEvents> {
        match event {
            AppEvents::Login(e) => {
                match e {
                    LoginEvents::Login2FAFailed(err) => {
                        ctx.set_error("Failed t login", err);
                        self.submitting = false;
                    }
                    LoginEvents::LoginSuccess(r) => match r {
                        Ok(r) => {
                            ctx.state_mut().login_state = LoginState::LoggedIn(r);
                            ctx.pop_and_push_view(ConversationView::new())
                        }
                        Err(e) => {
                            ctx.set_error("Failed to prepare user data", e);
                        }
                    },
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
                    self.do_submit(ctx);
                }
                KeyCode::Esc => {
                    ctx.state_mut().logout();
                    ctx.pop_view()
                }
                _ => {
                    self.input.handle_event(event);
                }
            }
        }
    }

    fn name(&self) -> &'static str {
        "Totp"
    }
}

impl TotpView {
    fn do_submit(&mut self, ctx: &mut AppViewContext) {
        if self.submitting {
            return;
        }
        self.submitting = true;
        let dispatcher = ctx.dispatcher();
        let code = self.input.value().to_string();
        ctx.state().submit_2fa(dispatcher, code);
    }
}
