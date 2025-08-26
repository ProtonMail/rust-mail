pub mod background;
pub mod contacts;
pub mod context_init;
pub mod login;
pub mod mailbox;
pub mod path_select_popup;
pub mod session_select;
pub mod twofa;
mod watcher;

use crate::CLI_ARGS;
use crate::app::{Command, Model};
use crate::app_model::background::BackgroundModel;
use crate::app_model::contacts::ContactsModel;
use crate::app_model::context_init::ContextInitModel;
use crate::app_model::login::LoginModel;
use crate::app_model::mailbox::MailboxModel;
use crate::app_model::path_select_popup::PathSelectPopup;
use crate::app_model::twofa::TwoFaModel;
use crate::keychain::AppKeyChain;
use crate::messages::Messages;
use crate::widgets::utils::ScrollableState;
use crate::widgets::{Backdrop, ScrollableParagraph};
use crate::widgets::{ScrollableListState, ScrollableParagraphState};
use anyhow::anyhow;
use chrono::Local;
use futures::FutureExt;
use proton_core_common::event_loop::EventPollMode;
use proton_core_common::services::SessionObserverService;
use proton_core_common::{OnSessionDeletedResponse, Origin};
use proton_log_service::LogService;
use proton_log_service::WorkerGuard;
use proton_mail_common::MailContext;
use ratatui::crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Flex};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, Paragraph, Row, Table, Wrap};
use session_select::SessionSelectModel;
use std::backtrace::Backtrace;
use std::fs::read_to_string;
use std::panic::{set_hook, take_hook};
use std::sync::Arc;
use std::time::Duration;
use throbber_widgets_tui::ThrobberState;
use tokio::runtime;
use tracing::error;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};
use tui_logger::{TuiLoggerLevelOutput, TuiWidgetState};

pub const APP_ID: &str = "com.proton.proton-mail-tui";

/// Internal application state.
pub enum AppState {
    /// There are existing sessions available, allow user to select one.
    SessionSelect(SessionSelectModel),
    /// Log into a new account.
    Login(LoginModel),
    /// Submit 2FA code.
    TwoFA(TwoFaModel),
    /// Initialize the user context.
    ContextInit(ContextInitModel),
    /// Display conversation/messages.
    Mailbox(MailboxModel),
    /// Display contacts and groups
    Contacts(ContactsModel),
    /// Background Execution Simulator
    Background(BackgroundModel),
}

/// Trait to enforce behavior on each of the app states.
pub trait AppStateHandler {
    /// Called when we enter this state.
    fn on_state_enter(&mut self) -> Command<Messages> {
        Command::None
    }

    /// Called when there is an input event.
    fn handle_event(&mut self, event: Event) -> Command<Messages>;
    /// Called when there is a message to be handled.
    fn update(&mut self, ctx: &Arc<MailContext>, message: Messages) -> Command<Messages>;
    /// Called to display the current state.
    fn view(&mut self, frame: &mut Frame, area: Rect);

    /// What will get shown on the help popup (F1)
    fn help_options(&self) -> Vec<(&'static str, &'static str)>;

    /// Called to provide contextual help that is displayed at the top.
    fn view_help_bar(&mut self, frame: &mut Frame, area: Rect);

    /// How many lines the help bar is.
    fn help_bar_lines(&self) -> u16 {
        1
    }
    /// Called to provide information it the status bar at the bottom.
    fn view_status_bar(&mut self, frame: &mut Frame, area: Rect);
}

/// Behavior for an application popup that will be displayed over the existing views.
///
/// Unlike [`AppStateHandler`], popups can only react to input and can not change their state.
pub trait Popup {
    fn title(&self) -> Option<String>;
    fn handle_event(&mut self, _: Event) -> Command<Messages> {
        Command::None
    }
    fn view(&mut self, frame: &mut Frame, area: Rect);
    fn height(&self) -> Constraint {
        Constraint::Percentage(60)
    }
    fn width(&self) -> Constraint {
        Constraint::Percentage(60)
    }
}

pub struct AppModel {
    context: Arc<MailContext>,
    state: AppState,
    popup: Option<Box<dyn Popup>>,
    bg_progress: Option<BackgroundProgress>,
    tui_logger_state: TuiWidgetState,
    display_log: bool,
    _log_guard: WorkerGuard,
    pending_popups: Vec<Box<dyn Popup>>,
}

impl AppModel {
    pub async fn new() -> anyhow::Result<Self> {
        let app_config = &CLI_ARGS;
        let cache_dir = dirs::cache_dir()
            .ok_or(anyhow!("Failed to get cache dir"))?
            .join(APP_ID)
            .join(app_config.dir());

        let data_dir = dirs::data_local_dir()
            .ok_or(anyhow!("Failed to get config dir"))?
            .join(APP_ID)
            .join(app_config.dir());

        let user_db_path = cache_dir.join("users");
        let mail_cache_dir = cache_dir.join("mail");
        let core_cache_dir = cache_dir.join("core");

        std::fs::create_dir_all(&cache_dir)?;
        std::fs::create_dir_all(&data_dir)?;
        std::fs::create_dir_all(&mail_cache_dir)?;
        std::fs::create_dir_all(&core_cache_dir)?;
        std::fs::create_dir_all(&user_db_path)?;

        let config = proton_log_service::Config::builder()
            .name("app".into())
            .directory(cache_dir.clone())
            .header(|| format!("\n--- Proton Mail TUI ---- Started at {}\n", Local::now()))
            .build();
        let log_service = LogService::new(config);
        let log_guard = init_log(&log_service)?;

        tracing::info!("Creating Async Runtime...");

        let mut keychain = AppKeyChain::new()?;
        keychain.init()?;

        let context = MailContext::new(
            Origin::App,
            runtime::Handle::current(),
            data_dir,
            user_db_path,
            core_cache_dir,
            mail_cache_dir,
            1 << 25, // 32MiB
            Arc::new(keychain),
            app_config.api_config(),
            None, // TODO(jhoulahan): Support HV challenge (at least sms/email)
            None, // TODO: Add DeviceInfoProvider support for mail-tui.
            log_service,
            EventPollMode::Automatic(Duration::from_secs(CLI_ARGS.event_loop_time.unwrap_or(15))),
            proton_network_monitor_service::Config::default(),
        )
        .await?;

        let sessions_model = SessionSelectModel::new(&context).await?;

        Ok(Self {
            context,
            state: AppState::SessionSelect(sessions_model),
            popup: None,
            bg_progress: None,
            tui_logger_state: TuiWidgetState::new(),
            display_log: false,
            _log_guard: log_guard,
            pending_popups: vec![],
        })
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
    fn on_ready(&mut self) -> Command<Messages> {
        let ready_cmd = self.state.on_state_enter();

        let ctx = self.context.clone();
        let session_observer_cmd = Command::background_task(move |sender| {
            async move {
                let session_service = ctx.core_context().get_service::<SessionObserverService>();
                session_service.on_session_deleted(move |_, user_id| {
                    let sender = sender.clone();
                    async move {
                        let _ = sender
                            .send_async(Command::message(Messages::SessionExpired(user_id)))
                            .await;
                        OnSessionDeletedResponse::Continue
                    }
                });
            }
            .boxed()
        });

        Command::batch([session_observer_cmd, ready_cmd])
    }

    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        if self.bg_progress.is_some() {
            return Command::None;
        }

        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press && key.code == KeyCode::F(2) {
                self.display_log = !self.display_log;
                return Command::None;
            }
        }

        if let Some(popup) = &mut self.popup {
            if let Event::Key(key) = &event {
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Esc {
                    return Command::message(Messages::DismissPopup);
                }
            }

            return popup.handle_event(event);
        }

        self.state.handle_event(event)
    }

    fn update(&mut self, message: Messages) -> Command<Messages> {
        let message = match message {
            Messages::DisplayBackgroundProgress(text) => {
                self.bg_progress = Some(BackgroundProgress::new(text));
                return Command::None;
            }
            Messages::DismissBackgroundProgress => {
                self.bg_progress = None;
                return Command::None;
            }
            Messages::DisplayError(title, error) => {
                let popup = InfoDialog::new_error(title, error);
                return Command::message(Messages::raise_popup(popup));
            }
            Messages::DisplayInfo(title, text) => {
                let popup = InfoDialog::new_info(title, text);
                return Command::message(Messages::raise_popup(popup));
            }
            Messages::DismissPopup => {
                self.popup = None;
                if !self.pending_popups.is_empty() {
                    self.popup = Some(self.pending_popups.remove(0));
                }
                return Command::None;
            }
            Messages::RaisePopup(popup) => {
                if self.popup.is_some() {
                    self.pending_popups.push(popup);
                } else {
                    self.popup = Some(popup);
                }
                return Command::None;
            }
            Messages::SelectFilePathPopup(closure) => {
                //TODO: Popups do not need sync requirement, but this library can't be made
                // sync.
                let popup = Box::new(PathSelectPopup::new(closure));
                if self.popup.is_some() {
                    self.pending_popups.push(popup);
                } else {
                    self.popup = Some(popup);
                }
                return Command::None;
            }
            Messages::SwitchAppState(new_state) => {
                self.state = new_state;
                return self.state.on_state_enter();
            }
            Messages::SessionExpired(ref user_id) => {
                if let Some(ctx) = match &self.state {
                    AppState::ContextInit(state) => Some(state.ctx()),
                    AppState::Mailbox(state) => Some(state.ctx()),
                    AppState::Contacts(state) => Some(state.ctx()),
                    AppState::Background(state) => Some(state.ctx()),
                    _ => None,
                } {
                    if *ctx.user_id() == *user_id {
                        let ctx = self.context.clone();
                        return Command::task(async move {
                            match SessionSelectModel::new(&ctx).await {
                                Ok(m) => Command::message(Messages::SwitchAppState(
                                    AppState::SessionSelect(m),
                                )),
                                Err(e) => Command::message(Messages::DisplayError(
                                    Some("Irrecoverable Error".to_string()),
                                    anyhow!("Failed to load session select state: {e:?}"),
                                )),
                            }
                        });
                    }
                }
                message
            }
            _ => message,
        };

        // TODO: use async commands to perform async queries
        #[allow(clippy::large_futures)]
        self.state.update(&self.context, message)
    }

    fn view(&mut self, frame: &mut Frame) {
        let area = frame.area();

        let help_lines = self.state.help_bar_lines();

        let [help_area, view_area, status_bar_area] = Layout::vertical([
            Constraint::Length(help_lines),
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
            frame.render_widget(Backdrop, frame.area());
            bg_progress.draw(frame);
        }

        if let Some(popup) = &mut self.popup {
            let [box_area] = Layout::vertical([popup.height()])
                .flex(Flex::Center)
                .areas(area);

            let [box_area] = Layout::horizontal([popup.width()])
                .flex(Flex::Center)
                .areas(box_area);

            let popup_area = box_area.inner(Margin {
                horizontal: 1,
                vertical: 1,
            });

            frame.render_widget(Backdrop, frame.area());
            frame.render_widget(Clear, box_area);
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
    fn on_state_enter(&mut self) -> Command<Messages> {
        match self {
            AppState::SessionSelect(state) => state.on_state_enter(),
            AppState::Login(state) => state.on_state_enter(),
            AppState::TwoFA(state) => state.on_state_enter(),
            AppState::ContextInit(state) => state.on_state_enter(),
            AppState::Mailbox(state) => state.on_state_enter(),
            AppState::Contacts(state) => state.on_state_enter(),
            AppState::Background(state) => state.on_state_enter(),
        }
    }

    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        if let Event::Key(k) = event {
            if k.code == KeyCode::F(1) {
                return Command::Message(Messages::RaisePopup(Box::new(HelpPopup {
                    items: self.help_options(),
                    list_state: ScrollableListState::new(Some(0)),
                })));
            }
        }
        match self {
            AppState::SessionSelect(state) => state.handle_event(event),
            AppState::Login(state) => state.handle_event(event),
            AppState::TwoFA(state) => state.handle_event(event),
            AppState::ContextInit(state) => state.handle_event(event),
            AppState::Mailbox(state) => state.handle_event(event),
            AppState::Contacts(state) => state.handle_event(event),
            AppState::Background(state) => state.handle_event(event),
        }
    }

    fn update(&mut self, ctx: &Arc<MailContext>, message: Messages) -> Command<Messages> {
        match self {
            AppState::SessionSelect(state) => state.update(ctx, message),
            AppState::Login(state) => state.update(ctx, message),
            AppState::TwoFA(state) => state.update(ctx, message),
            AppState::ContextInit(state) => state.update(ctx, message),
            AppState::Mailbox(state) => state.update(ctx, message),
            AppState::Contacts(state) => state.update(ctx, message),
            AppState::Background(state) => state.update(ctx, message),
        }
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        match self {
            AppState::SessionSelect(state) => state.view(frame, area),
            AppState::Login(state) => state.view(frame, area),
            AppState::TwoFA(state) => state.view(frame, area),
            AppState::ContextInit(state) => state.view(frame, area),
            AppState::Mailbox(state) => state.view(frame, area),
            AppState::Contacts(state) => state.view(frame, area),
            AppState::Background(state) => state.view(frame, area),
        }
    }

    fn view_help_bar(&mut self, frame: &mut Frame, area: Rect) {
        match self {
            AppState::SessionSelect(state) => state.view_help_bar(frame, area),
            AppState::Login(state) => state.view_help_bar(frame, area),
            AppState::TwoFA(state) => state.view_help_bar(frame, area),
            AppState::ContextInit(state) => state.view_help_bar(frame, area),
            AppState::Mailbox(state) => state.view_help_bar(frame, area),
            AppState::Contacts(state) => state.view_help_bar(frame, area),
            AppState::Background(state) => state.view_help_bar(frame, area),
        }
    }

    fn view_status_bar(&mut self, frame: &mut Frame, area: Rect) {
        match self {
            AppState::SessionSelect(state) => state.view_status_bar(frame, area),
            AppState::Login(state) => state.view_status_bar(frame, area),
            AppState::TwoFA(state) => state.view_status_bar(frame, area),
            AppState::ContextInit(state) => state.view_status_bar(frame, area),
            AppState::Mailbox(state) => state.view_status_bar(frame, area),
            AppState::Contacts(state) => state.view_status_bar(frame, area),
            AppState::Background(state) => state.view_status_bar(frame, area),
        }
    }

    fn help_bar_lines(&self) -> u16 {
        match self {
            AppState::SessionSelect(state) => state.help_bar_lines(),
            AppState::Login(state) => state.help_bar_lines(),
            AppState::TwoFA(state) => state.help_bar_lines(),
            AppState::ContextInit(state) => state.help_bar_lines(),
            AppState::Mailbox(state) => state.help_bar_lines(),
            AppState::Contacts(state) => state.help_bar_lines(),
            AppState::Background(state) => state.help_bar_lines(),
        }
    }

    fn help_options(&self) -> Vec<(&'static str, &'static str)> {
        match self {
            AppState::SessionSelect(state) => state.help_options(),
            AppState::Login(state) => state.help_options(),
            AppState::TwoFA(state) => state.help_options(),
            AppState::ContextInit(state) => state.help_options(),
            AppState::Mailbox(state) => state.help_options(),
            AppState::Contacts(state) => state.help_options(),
            AppState::Background(state) => state.help_options(),
        }
    }
}

fn app_tracing_env_filter(trace: bool) -> EnvFilter {
    let log_level = if trace { "trace" } else { "debug" };
    let directives = read_to_string("log_directives");
    let directives: String = directives
        .unwrap_or(format!(
            "info,
        proton_mail_tui=debug,
        proton_core_api={log_level},
        proton_sqlite3={log_level},
        proton_core_common={log_level},
        proton_mail_common={log_level},
        proton_event_loop={log_level},
        proton_action_queue={log_level},
        proton_calendar_common={log_level},
        proton_network_monitor_service=debug,\
        {}",
            LogService::silence_muon_errors_evn_filter()
        ))
        .split_inclusive(',')
        .map(str::trim)
        .collect();
    EnvFilter::builder()
        .with_default_directive(LevelFilter::TRACE.into())
        .parse(directives)
        .expect("Error parsing tracing directives")
}

fn init_log(log_service: &LogService) -> anyhow::Result<WorkerGuard> {
    let (file_subscriber, guard) = log_service.create_non_blocking_layer()?;
    let file_subscriber = file_subscriber
        .with_file(false)
        .with_line_number(false)
        .with_target(false)
        .with_ansi(false)
        .with_filter(app_tracing_env_filter(CLI_ARGS.trace_logs));
    let tui_log_subscriber = tui_logger::tracing_subscriber_layer()
        .with_filter(app_tracing_env_filter(CLI_ARGS.trace_logs));
    tui_logger::set_default_level(log::LevelFilter::Debug);
    tracing_subscriber::registry()
        .with(file_subscriber)
        .with(tui_log_subscriber)
        .init();
    log_backtrace_on_panic();
    Ok(guard)
}

/// Modify the hook on panic so we log the `Backtrace`.
fn log_backtrace_on_panic() {
    let previous_hook = take_hook();
    set_hook(Box::new(move |info| {
        error!("Backtrace: {info}\n{}", Backtrace::force_capture());
        previous_hook(info);
    }));
}

pub struct InfoDialog {
    title: Option<String>,
    text: ScrollableParagraph<'static>,
    state: ScrollableParagraphState,
}

impl InfoDialog {
    #[allow(clippy::needless_pass_by_value)]
    pub fn new_error(title: Option<String>, err: anyhow::Error) -> Self {
        let title = Some(title.unwrap_or_else(|| "Error".to_owned()));

        Self::new(
            title,
            format!("{err:?}")
                .lines()
                .map(|line| line.to_owned().on_red())
                .collect::<Text>(),
        )
    }

    pub fn new_info(title: Option<String>, text: impl Into<Text<'static>>) -> Self {
        Self::new(title, text.into())
    }

    fn new(title: Option<String>, mut text: Text<'static>) -> Self {
        text.lines.push(Line::raw(""));

        text.lines
            .push(Line::raw("Press any key to continue...").bold());

        Self {
            text: ScrollableParagraph(Paragraph::new(text).wrap(Wrap { trim: false })),
            state: ScrollableParagraphState::default(),
            title,
        }
    }
}

#[allow(clippy::cast_possible_truncation)]
impl Popup for InfoDialog {
    fn title(&self) -> Option<String> {
        self.title.clone()
    }

    fn handle_event(&mut self, ev: Event) -> Command<Messages> {
        let Event::Key(key) = ev else {
            return Command::None;
        };

        if self.state.handle_event(key.code) {
            Command::None
        } else {
            Command::message(Messages::DismissPopup)
        }
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_stateful_widget(self.text.clone(), area, &mut self.state);
    }

    fn height(&self) -> Constraint {
        Constraint::Percentage(40)
    }

    fn width(&self) -> Constraint {
        Constraint::Percentage(60)
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
        let area = frame.area();
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
        .areas(content.inner(Margin {
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

/// Simple confirmation popup.
pub struct YesNoPopup {
    title: String,
    description: String,
    accept_command: Option<Command<Messages>>,
    reject_command: Option<Command<Messages>>,
}

impl YesNoPopup {
    /// Create new instance with a `title` and a `description`;
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
            accept_command: None,
            reject_command: None,
        }
    }

    /// Set the command that should be executed when the prompt is accepted.
    pub fn on_accept(mut self, command: impl Into<Command<Messages>>) -> Self {
        self.accept_command = Some(command.into());
        self
    }

    /// Set the command that should be executed when the prompt is rejected.
    #[allow(dead_code)]
    pub fn on_reject(mut self, command: impl Into<Command<Messages>>) -> Self {
        self.reject_command = Some(command.into());
        self
    }
}

impl Popup for YesNoPopup {
    fn title(&self) -> Option<String> {
        Some(self.title.clone())
    }

    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        if let Event::Key(key) = event {
            match key.code {
                KeyCode::Esc | KeyCode::Char('n' | 'N') => {
                    return Command::batch([
                        Command::message(Messages::DismissPopup),
                        self.reject_command.take().unwrap_or_default(),
                    ]);
                }
                KeyCode::Char('y' | 'Y') => {
                    return Command::batch([
                        Command::message(Messages::DismissPopup),
                        self.accept_command.take().unwrap_or_default(),
                    ]);
                }
                _ => {}
            }
        }

        Command::None
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let [_, msg, _, instructions] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Min(3),
            Constraint::Fill(1),
            Constraint::Length(2),
        ])
        .flex(Flex::Center)
        .areas(area.inner(Margin::new(2, 2)));
        frame.render_widget(Block::new(), area);
        frame.render_widget(
            Paragraph::new(self.description.to_string())
                .centered()
                .wrap(Wrap { trim: false }),
            msg,
        );
        frame.render_widget(
            Text::from(Line::from(vec![
                Span::from("Y/y:").bold(),
                Span::from(" Accept "),
                Span::from("            "),
                Span::from("Esc/N/n:").bold(),
                Span::from(" Reject"),
            ]))
            .centered()
            .white()
            .bold(),
            instructions,
        );
    }
}

pub struct HelpPopup {
    items: Vec<(&'static str, &'static str)>,
    list_state: ScrollableListState,
}

impl Popup for HelpPopup {
    fn title(&self) -> Option<String> {
        Some("Help".into())
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let Some(max) = self.items.iter().map(|x| x.0.len()).max() else {
            error!("Help list was somehow emtpy");
            return;
        };

        let max: u16 = max.try_into().unwrap();

        let rows = self.items.iter().map(|(key, desc)| Row::new([*key, *desc]));

        let table = Table::new(rows, [Constraint::Length(max), Constraint::Fill(1)]);
        frame.render_widget(table, area);
    }

    fn height(&self) -> Constraint {
        let len: u16 = self.items.len().try_into().unwrap();
        Constraint::Length(len + 2) // +2 to account for margins
    }

    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        let Event::Key(key) = event else {
            return Command::None;
        };
        if self.list_state.handle_event(key.code) {
            return Command::None;
        }
        Command::message(Messages::DismissPopup)
    }
}

#[derive(Default)]
pub struct ChoosePopup<T> {
    width: u16,
    height: u16,
    widgets: Vec<ChoosePopupWidget<T>>,
    handler: Option<Box<dyn FnOnce(T) -> Command<Messages> + Send>>,
}

impl<T> ChoosePopup<T> {
    pub fn with(mut self, key: KeyCode, label: impl Into<String>, event: T) -> Self {
        let label = label.into();

        self.width = self
            .width
            .max(u16::try_from(key.desc().len() + 1 + label.len()).unwrap());

        self.height += 1;

        self.widgets.push(ChoosePopupWidget::Button {
            key,
            label,
            event: Some(event),
        });

        self
    }

    pub fn space(mut self) -> Self {
        self.height += 1;
        self.widgets.push(ChoosePopupWidget::Space);
        self
    }

    pub fn on_reply(
        mut self,
        handler: impl FnOnce(T) -> Command<Messages> + Send + 'static,
    ) -> Self {
        self.handler = Some(Box::new(handler));
        self
    }
}

impl<T> Popup for ChoosePopup<T> {
    fn title(&self) -> Option<String> {
        None
    }

    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        if let Event::Key(pressed) = event {
            if pressed.modifiers.is_empty() {
                let event = self.widgets.iter_mut().find_map(|widget| {
                    let ChoosePopupWidget::Button { key, event, .. } = widget else {
                        return None;
                    };

                    if *key == pressed.code {
                        event.take()
                    } else {
                        None
                    }
                });

                if let (Some(event), Some(handler)) = (event, self.handler.take()) {
                    return handler(event);
                }
            }
        }

        Command::None
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let body = List::new(self.widgets.iter().map(|widget| widget.as_line()));
        let area = area.inner(Margin::new(1, 0));

        frame.render_widget(body, area);
    }

    fn height(&self) -> Constraint {
        Constraint::Length(self.height + 2)
    }

    fn width(&self) -> Constraint {
        Constraint::Length(self.width + 4)
    }
}

#[derive(Debug)]
enum ChoosePopupWidget<T> {
    Button {
        key: KeyCode,
        label: String,
        event: Option<T>,
    },
    Space,
}

impl<T> ChoosePopupWidget<T> {
    fn as_line(&self) -> Line<'_> {
        match self {
            ChoosePopupWidget::Button { key, label, .. } => Line::from_iter([
                Span::raw(key.desc()).bold(),
                Span::raw(" "),
                Span::raw(label),
            ]),

            ChoosePopupWidget::Space => Line::default(),
        }
    }
}

pub trait KeyCodeExt {
    fn desc(&self) -> String;
}

impl KeyCodeExt for KeyCode {
    fn desc(&self) -> String {
        match self {
            KeyCode::Esc => "Esc:".into(),
            KeyCode::Char(ch) => format!("{ch}:"),
            this => unimplemented!("don't know how to describe {this:?}"),
        }
    }
}
