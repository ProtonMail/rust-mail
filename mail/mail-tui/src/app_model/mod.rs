pub mod context_init;
pub mod login;
pub mod mailbox;
pub mod session_select;
pub mod twofa;

use crate::app::Model;
use crate::keychain::AppKeyChain;
use crate::messages::Messages;
use anyhow::anyhow;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use proton_async::runtime;
use proton_async::sync::mpsc::Sender;
use proton_mail_common::exports::tracing;
use proton_mail_common::exports::tracing::level_filters::LevelFilter;
use proton_mail_common::proton_api_mail::proton_api_core::http::Builder;
use proton_mail_common::MailContext;
use ratatui::layout::Flex;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use throbber_widgets_tui::ThrobberState;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};
use tui_logger::{TuiLoggerLevelOutput, TuiWidgetState};

pub const APP_ID: &str = "com.proton.proton-mail-tui";

/// Internal application state.
pub enum AppState {
    /// There are existing sessions available, allow user to select one.
    SessionSelect(session_select::Model),
    /// Log into a new account.
    Login(login::Model),
    /// Submit 2FA code.
    TwoFA(twofa::Model),
    /// Initialize the user context.
    ContextInit(context_init::Model),
    /// Display conversation/messages.
    Mailbox(mailbox::Model),
}

/// Convenience wrapper which logs errors.
///
/// Errors should only occur if the application has been terminated before a background task.
#[derive(Clone)]
pub struct BackgroundSender(Sender<Messages>);

impl BackgroundSender {
    pub fn send(&self, msg: Messages) {
        if self.0.send(msg).is_err() {
            tracing::error!("Failed to send message, channel may be closed");
        }
    }

    #[allow(unused)]
    pub async fn send_async(&self, msg: Messages) {
        if self.0.send_async(msg).await.is_err() {
            tracing::error!("Failed to send message, channel may be closed");
        }
    }
}

/// Trait to enforce behavior on each of the app states.
pub trait AppStateHandler {
    /// Called when we enter this state.
    fn on_state_enter(&mut self) -> Option<Messages> {
        None
    }

    /// Called when there is an input event.
    fn handle_event(&mut self, event: Event) -> Option<Messages>;
    /// Called when there is a message to be handled.
    fn update(
        &mut self,
        ctx: &MailContext,
        message: Messages,
        sender: &BackgroundSender,
    ) -> Option<Messages>;
    /// Called to display the current state.
    fn view(&mut self, frame: &mut Frame, area: Rect);

    /// Called to provide contextual help that is displayed at the top.
    fn view_help_bar(&mut self, frame: &mut Frame, area: Rect);
    /// Called to provide information it the status bar at the bottom.
    fn view_status_bar(&mut self, frame: &mut Frame, area: Rect);
}

/// Behavior for an application popup that will be displayed over the existing views.
///
/// Unlike [`AppStateHandler`], popups can only react to input and can not change their state.
pub trait Popup {
    /// Popup title to be drawn around the box.
    fn title(&self) -> Option<String>;
    /// Handle input event.
    fn handle_event(&mut self, _: Event) -> Option<Messages> {
        None
    }
    /// Display popup contents.
    fn view(&mut self, frame: &mut Frame, area: Rect);
}

pub struct AppModel {
    context: MailContext,
    state: AppState,
    popup: Option<Box<dyn Popup>>,
    bg_progress: Option<BackgroundProgress>,
    tui_logger_state: TuiWidgetState,
    display_log: bool,
}

impl AppModel {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let cache_dir = dirs::cache_dir()
            .ok_or(anyhow!("Failed to get cache dir"))?
            .join(APP_ID);

        let config_dir = dirs::config_dir()
            .ok_or(anyhow!("Failed to get config dir"))?
            .join(APP_ID);

        let user_db_path = cache_dir.join("users");
        let mail_cache_dir = cache_dir.join("mail");

        std::fs::create_dir_all(&cache_dir)?;
        std::fs::create_dir_all(&config_dir)?;
        std::fs::create_dir_all(&mail_cache_dir)?;
        std::fs::create_dir_all(&user_db_path)?;

        let log_file = cache_dir.join("app.log");
        init_log(log_file)?;

        tracing::info!("Creating Async Runtime...");
        let mut keychain = AppKeyChain::new()?;
        keychain.init()?;
        let runtime = runtime::MultiThreaded::new(4)?;
        let client = Builder::new().build()?;
        let context = MailContext::new(
            runtime,
            config_dir,
            user_db_path,
            mail_cache_dir,
            Arc::new(keychain),
            client,
            None,
        )?;

        let sessions_model = session_select::Model::new(&context)?;
        Ok(Self {
            context,
            state: AppState::SessionSelect(sessions_model),
            popup: None,
            bg_progress: None,
            tui_logger_state: TuiWidgetState::new(),
            display_log: false,
        })
    }

    pub fn set_error(&mut self, title: impl Into<String>, error: impl Into<anyhow::Error>) {
        self.popup = Some(Box::new(ErrorDialog::new(title.into(), error.into())));
    }

    fn render_log(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_widget(
            tui_logger::TuiLoggerWidget::default()
                .style_error(Style::default().fg(Color::Red))
                .style_debug(Style::default().fg(Color::Green))
                .style_warn(Style::default().fg(Color::Yellow))
                .style_trace(Style::default().fg(Color::Black))
                .style_info(Style::default().fg(Color::White))
                .style(Style::default().bg(Color::Black))
                .output_separator(':')
                .output_timestamp(Some("%H:%M:%S".to_string()))
                .output_level(Some(TuiLoggerLevelOutput::Abbreviated))
                .output_target(true)
                .output_file(false)
                .output_line(false)
                .state(&self.tui_logger_state),
            area,
        );
    }
}

impl Model<Messages> for AppModel {
    fn on_ready(&mut self) -> Option<Messages> {
        self.state.on_state_enter()
    }

    fn handle_event(&mut self, event: Event) -> Option<Messages> {
        if self.bg_progress.is_some() {
            return None;
        }

        if let Event::Key(key) = &event {
            if key.kind == KeyEventKind::Press && key.code == KeyCode::F(2) {
                self.display_log = !self.display_log;
                return None;
            }
        }

        if let Some(popup) = &mut self.popup {
            if let Event::Key(key) = &event {
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Esc {
                    return Some(Messages::DismissPopup);
                }
            }

            let message = popup.handle_event(event);
            // Close popup if a message is returned.
            if message.is_some() {
                self.popup = None;
            }
            return message;
        }

        self.state.handle_event(event)
    }

    fn update(&mut self, message: Messages, sender: &Sender<Messages>) -> Option<Messages> {
        let message = match message {
            Messages::DisplayBackgroundProgress(text) => {
                self.bg_progress = Some(BackgroundProgress::new(text));
                return None;
            }
            Messages::DismissBackgroundProgress => {
                self.bg_progress = None;
                return None;
            }
            Messages::DisplayError(title, error) => {
                self.set_error(title.unwrap_or("Error".to_owned()), error);
                return None;
            }
            Messages::DismissPopup => {
                self.popup = None;
                return None;
            }
            Messages::RaisePopup(popup) => {
                if self.popup.is_some() {
                    tracing::warn!("Raising new popup over existing");
                }
                self.popup = Some(popup);
                return None;
            }
            Messages::SwitchAppState(new_state) => {
                self.state = new_state;
                return self.state.on_state_enter();
            }
            _ => message,
        };

        let sender = BackgroundSender(sender.clone());
        self.state.update(&self.context, message, &sender)
    }

    fn view(&mut self, frame: &mut Frame) {
        let area = frame.size();
        let [help_area, view_area, status_bar_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Percentage(100),
            Constraint::Length(1),
        ])
        .areas(area);

        // Draw help bar
        frame.render_widget(Block::new().style(Style::new().reversed()), help_area);

        // Draw status bar
        frame.render_widget(Block::new().style(Style::new().reversed()), status_bar_area);
        let [title_area, view_status_area] =
            Layout::horizontal([Constraint::Length(18), Constraint::Fill(1)])
                .areas(status_bar_area);
        let text = Text::from("Proton Mail TUI | ".bold());
        frame.render_widget(text, title_area);

        if self.display_log {
            self.render_log(frame, view_area);
            frame.render_widget(Text::from("Log"), view_status_area);
            return;
        }
        self.state.view_help_bar(frame, help_area);
        self.state.view(frame, view_area);
        self.state.view_status_bar(frame, view_status_area);

        if let Some(bg_progress) = &mut self.bg_progress {
            bg_progress.draw(frame);
        }
        if let Some(popup) = &mut self.popup {
            let [_, box_area, _] = Layout::vertical([
                Constraint::Percentage(20),
                Constraint::Percentage(60),
                Constraint::Percentage(20),
            ])
            .flex(Flex::SpaceAround)
            .areas(area);
            let [_, box_area, _] = Layout::horizontal([
                Constraint::Percentage(20),
                Constraint::Percentage(60),
                Constraint::Percentage(20),
            ])
            .flex(Flex::SpaceAround)
            .areas(box_area);
            let popup_area = box_area.inner(&Margin {
                horizontal: 1,
                vertical: 1,
            });
            frame.render_widget(Clear {}, box_area);
            popup.view(frame, popup_area);
            let mut block = Block::new().borders(Borders::ALL);
            if let Some(title) = popup.title() {
                block = block.title(title);
            }
            frame.render_widget(block, box_area);
        }
    }
}

impl AppStateHandler for AppState {
    fn on_state_enter(&mut self) -> Option<Messages> {
        match self {
            AppState::SessionSelect(state) => state.on_state_enter(),
            AppState::Login(state) => state.on_state_enter(),
            AppState::TwoFA(state) => state.on_state_enter(),
            AppState::ContextInit(state) => state.on_state_enter(),
            AppState::Mailbox(state) => state.on_state_enter(),
        }
    }

    fn handle_event(&mut self, event: Event) -> Option<Messages> {
        match self {
            AppState::SessionSelect(state) => state.handle_event(event),
            AppState::Login(state) => state.handle_event(event),
            AppState::TwoFA(state) => state.handle_event(event),
            AppState::ContextInit(state) => state.handle_event(event),
            AppState::Mailbox(state) => state.handle_event(event),
        }
    }

    fn update(
        &mut self,
        ctx: &MailContext,
        message: Messages,
        sender: &BackgroundSender,
    ) -> Option<Messages> {
        match self {
            AppState::SessionSelect(state) => state.update(ctx, message, sender),
            AppState::Login(state) => state.update(ctx, message, sender),
            AppState::TwoFA(state) => state.update(ctx, message, sender),
            AppState::ContextInit(state) => state.update(ctx, message, sender),
            AppState::Mailbox(state) => state.update(ctx, message, sender),
        }
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        match self {
            AppState::SessionSelect(state) => state.view(frame, area),
            AppState::Login(state) => state.view(frame, area),
            AppState::TwoFA(state) => state.view(frame, area),
            AppState::ContextInit(state) => state.view(frame, area),
            AppState::Mailbox(state) => state.view(frame, area),
        }
    }

    fn view_help_bar(&mut self, frame: &mut Frame, area: Rect) {
        match self {
            AppState::SessionSelect(state) => state.view_help_bar(frame, area),
            AppState::Login(state) => state.view_help_bar(frame, area),
            AppState::TwoFA(state) => state.view_help_bar(frame, area),
            AppState::ContextInit(state) => state.view_help_bar(frame, area),
            AppState::Mailbox(state) => state.view_help_bar(frame, area),
        }
    }

    fn view_status_bar(&mut self, frame: &mut Frame, area: Rect) {
        match self {
            AppState::SessionSelect(state) => state.view_status_bar(frame, area),
            AppState::Login(state) => state.view_status_bar(frame, area),
            AppState::TwoFA(state) => state.view_status_bar(frame, area),
            AppState::ContextInit(state) => state.view_status_bar(frame, area),
            AppState::Mailbox(state) => state.view_status_bar(frame, area),
        }
    }
}

fn app_tracing_env_filter() -> EnvFilter {
    EnvFilter::builder()
        .with_default_directive(LevelFilter::TRACE.into())
        .parse_lossy(
            "info,proton_mail_tui=debug,proton_mail_db=trace,proton_sqlite3=trace,\
                    proton_core_db=trace,proton_core_common=trace,proton_mail_common=trace,\
                    proton_event_loop=trace,proton_api_core=trace,proton_action_queue=trace",
        )
}

fn init_log(log_path: impl AsRef<Path>) -> Result<(), Box<dyn Error>> {
    let log_file = std::fs::File::create(log_path)?;
    let file_subscriber = tracing_subscriber::fmt::layer()
        .with_file(false)
        .with_line_number(false)
        .with_writer(log_file)
        .with_target(false)
        .with_ansi(false)
        .with_filter(app_tracing_env_filter());
    let tui_log_subscriber =
        tui_logger::tracing_subscriber_layer().with_filter(app_tracing_env_filter());
    tui_logger::set_default_level(log::LevelFilter::Debug);
    tracing_subscriber::registry()
        .with(file_subscriber)
        .with(tui_log_subscriber)
        .init();
    Ok(())
}

struct ErrorDialog {
    error: anyhow::Error,
    source: String,
}

impl ErrorDialog {
    fn new(source: String, error: anyhow::Error) -> Self {
        Self { error, source }
    }
}

impl Popup for ErrorDialog {
    fn title(&self) -> Option<String> {
        Some(self.source.clone())
    }

    fn handle_event(&mut self, _: Event) -> Option<Messages> {
        Some(Messages::DismissPopup)
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let [_, msg, _, instructions] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Min(3),
            Constraint::Fill(1),
            Constraint::Length(2),
        ])
        .flex(Flex::Center)
        .areas(area.inner(&Margin::new(2, 2)));
        frame.render_widget(Block::new().white().on_red(), area);
        frame.render_widget(
            Paragraph::new(self.error.to_string())
                .centered()
                .white()
                .bold()
                .wrap(Wrap { trim: false }),
            msg,
        );
        frame.render_widget(
            Text::from("Press any key to continue...")
                .centered()
                .white()
                .bold(),
            instructions,
        );
    }
}

struct BackgroundProgress {
    text: String,
    state: ThrobberState,
}

impl BackgroundProgress {
    fn new(text: String) -> Self {
        Self {
            text,
            state: ThrobberState::default(),
        }
    }

    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.size();
        let [_, content, _] = Layout::vertical([
            Constraint::Percentage(50),
            Constraint::Length(3),
            Constraint::Percentage(50),
        ])
        .flex(Flex::SpaceAround)
        .areas(area);
        let [_, content, _] = Layout::horizontal([
            Constraint::Percentage(30),
            Constraint::Percentage(40),
            Constraint::Percentage(30),
        ])
        .flex(Flex::SpaceAround)
        .areas(content);

        frame.render_widget(Clear, content);
        let block = Block::new().borders(Borders::ALL);
        frame.render_widget(block, content);
        self.state.calc_next();

        let [_, spinner_area, _] = Layout::horizontal([
            Constraint::Percentage(50),
            Constraint::Min(10),
            Constraint::Percentage(50),
        ])
        .areas(content.inner(&Margin {
            horizontal: 1,
            vertical: 1,
        }));
        let full = throbber_widgets_tui::Throbber::default()
            .label(&self.text)
            .throbber_set(throbber_widgets_tui::BRAILLE_SIX)
            .use_type(throbber_widgets_tui::WhichUse::Spin);
        frame.render_stateful_widget(full, spinner_area, &mut self.state);
    }
}
