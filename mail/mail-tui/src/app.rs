use crate::queue::{DispatchQueue, LocalDispatchObject, QueueDispatcher};
use crate::style::background_style;
use crate::view::{View, ViewStack};
use crate::views::ErrorDialog;
use crate::widgets::{HelpView, HelpViewState};
use crate::TerminalType;
use crossterm::event;
use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Flex, Layout};
use ratatui::prelude::Stylize;
use ratatui::style::{Color, Style, Styled};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Paragraph};
use tui_logger::{TuiLoggerLevelOutput, TuiWidgetEvent, TuiWidgetState};

pub trait AppEventHandler<T: 'static + AppEventHandler<T, E>, E: 'static + Send> {
    fn on_event(&mut self, dispatcher: AppLocalDispatcher<T, E>, event: E);
}

pub struct AppContext<T: 'static + AppEventHandler<T, E>, E: 'static + Send> {
    state: T,
    dispatch_queue: DispatchQueue<App<T, E>>,
    local_queue: Vec<LocalDispatchObject<App<T, E>>>,
}

impl<T: 'static + AppEventHandler<T, E>, E: 'static + Send> AppContext<T, E> {
    pub fn state(&self) -> &T {
        &self.state
    }

    pub fn app_local_dispatcher(&mut self) -> AppLocalDispatcher<'_, T, E> {
        AppLocalDispatcher(&mut self.local_queue, &self.dispatch_queue)
    }

    fn handle_event(&mut self, event: E) {
        self.state.on_event(
            AppLocalDispatcher(&mut self.local_queue, &self.dispatch_queue),
            event,
        );
    }
}

pub struct AppLocalDispatcher<'a, T: 'static + AppEventHandler<T, E>, E: Send + 'static>(
    &'a mut Vec<LocalDispatchObject<App<T, E>>>,
    &'a DispatchQueue<App<T, E>>,
);

impl<'a, T: 'static + AppEventHandler<T, E>, E: 'static + Send> AppLocalDispatcher<'a, T, E> {
    pub fn set_error<Error: std::error::Error + 'static, S: Into<String>>(
        &mut self,
        desc: S,
        err: Error,
    ) {
        let desc = desc.into();
        self.0.push(Box::new(move |app| {
            app.set_error(desc, err);
        }));
    }

    #[allow(unused)]
    pub fn quit(&mut self) {
        self.0.push(Box::new(|app| {
            app.quit();
        }));
    }

    pub fn background_dispatcher(&self) -> AppBackgroundDispatcher<T, E> {
        AppBackgroundDispatcher::new(self.1.dispatcher().clone())
    }

    #[allow(unused)]
    pub fn queue(&mut self, f: impl FnOnce(&mut App<T, E>) + 'static) {
        self.0.push(Box::new(f));
    }

    #[allow(unused)]
    pub fn queue_event(&mut self, e: impl Into<E>) {
        self._queue_event(e.into())
    }

    pub fn _queue_event(&mut self, e: E) {
        self.0.push(Box::new(|app| app.events.push(e)))
    }

    pub fn push_view<V: View<AppContext<T, E>, E> + 'static>(&mut self, view: V) {
        self.0.push(Box::new(|app| app.push_view(view)));
    }

    #[allow(unused)]
    pub fn pop_all_views(&mut self) {
        self.0.push(Box::new(|app| app.pop_all_views()));
    }

    #[allow(unused)]
    pub fn pop_view(&mut self) {
        self.0.push(Box::new(|app| app.pop_view()));
    }
}
#[derive(Clone)]
pub struct AppBackgroundDispatcher<T: 'static + AppEventHandler<T, E>, E: 'static + Send>(
    QueueDispatcher<App<T, E>>,
);

impl<T: 'static + AppEventHandler<T, E>, E: 'static + Send> AppBackgroundDispatcher<T, E> {
    pub fn new(d: QueueDispatcher<App<T, E>>) -> Self {
        Self(d)
    }

    pub fn queue_event(&self, e: impl Into<E>) {
        self._queue_event(e.into())
    }
    pub fn _queue_event(&self, e: E) {
        self.0.queue_sync(|app| app.events.push(e));
    }

    pub async fn queue_event_async(&self, e: impl Into<E>) {
        self._queue_event_async(e.into()).await
    }

    pub async fn _queue_event_async(&self, e: E) {
        self.0.queue_async(|app| app.events.push(e)).await;
    }

    #[allow(unused)]
    pub fn queue_on_main(&self, f: impl FnOnce(&mut App<T, E>) + 'static + Send) {
        self.0.queue_sync(f);
    }

    #[allow(unused)]
    pub async fn queue_on_main_async(&self, f: impl FnOnce(&mut App<T, E>) + 'static + Send) {
        self.0.queue_async(f).await;
    }

    #[allow(unused)]
    pub fn set_error<Error: std::error::Error + Send + 'static, S: Into<String>>(
        &self,
        desc: S,
        err: Error,
    ) {
        let desc = desc.into();
        self.0.queue_sync(move |app| {
            app.set_error(desc, err);
        });
    }

    #[allow(unused)]
    pub async fn set_error_async<Error: std::error::Error + Send + 'static, S: Into<String>>(
        &self,
        desc: S,
        err: Error,
    ) {
        let desc = desc.into();
        self.0
            .queue_async(move |app| {
                app.set_error(desc, err);
            })
            .await;
    }
}

pub struct App<T: 'static + AppEventHandler<T, E>, E: 'static + Send> {
    ctx: AppContext<T, E>,
    views: ViewStack<AppContext<T, E>, E>,
    events: Vec<E>,
    error: Option<ErrorDialog>,
    help_view_state: HelpViewState,
    tui_logger_state: TuiWidgetState,
    quit: bool,
    display_help: bool,
    display_log: bool,
}

impl<T: Sized + 'static + AppEventHandler<T, E>, E: Send + 'static> App<T, E> {
    pub fn new(state: T) -> Self {
        Self {
            ctx: AppContext {
                state,
                dispatch_queue: DispatchQueue::new(),
                local_queue: Vec::with_capacity(8),
            },
            views: ViewStack::new(),
            events: Vec::with_capacity(8),
            error: None,
            help_view_state: HelpViewState::default(),
            display_help: false,
            quit: false,
            display_log: false,
            tui_logger_state: TuiWidgetState::new(),
        }
    }

    pub fn run(&mut self, mut terminal: TerminalType) -> Result<(), Box<dyn std::error::Error>> {
        while !self.quit {
            self.tick();
            self.draw(&mut terminal)?;
            self.poll_events()?;
        }
        Ok(())
    }

    pub fn push_view<V: View<AppContext<T, E>, E> + 'static>(&mut self, view: V) {
        self.views.push_view(view);
    }

    pub fn pop_view(&mut self) {
        self.views.pop_view();
    }

    pub fn pop_all_views(&mut self) {
        self.views.pop_all()
    }

    pub fn quit(&mut self) {
        self.quit = true;
    }

    pub fn set_error(
        &mut self,
        source: impl Into<String>,
        err: impl Into<Box<dyn std::error::Error>>,
    ) {
        self.error = Some(ErrorDialog::new(source.into(), err.into()));
    }

    fn tick(&mut self) {
        while let Some(object) = self.ctx.dispatch_queue.try_receive() {
            self.ctx.local_queue.push(object);
        }

        let queue = std::mem::take(&mut self.ctx.local_queue);
        for object in queue {
            (object)(self)
        }

        for event in self.events.drain(..) {
            self.ctx.handle_event(event);
        }

        self.views.process(&mut self.ctx);
    }

    fn draw(&mut self, terminal: &mut TerminalType) -> Result<(), Box<dyn std::error::Error>> {
        terminal.draw(|frame| {
            let block = Block::new()
                .borders(Borders::TOP)
                .set_style(background_style())
                .title(" Proton Mail TUI ");
            frame.render_widget(block, frame.size());

            let [_, content, help] = Layout::vertical([
                Constraint::Length(1),
                Constraint::Min(10),
                Constraint::Length(2),
            ])
            .areas(frame.size());
            let help_block = Block::new().borders(Borders::TOP).white();
            frame.render_widget(help_block, help);

            let [_, hcontent, _] = Layout::horizontal([
                Constraint::Length(1),
                Constraint::Min(8),
                Constraint::Length(1),
            ])
            .areas(content);
            let [_, help] =
                Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(help);
            let [_, help_area, _] = Layout::horizontal([
                Constraint::Length(1),
                Constraint::Min(20),
                Constraint::Length(1),
            ])
            .flex(Flex::Center)
            .areas(help);

            if self.display_log {
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
                    hcontent,
                );
                frame.render_widget(
                    Text::from(Line::from(vec!["F2 ".bold(), "Close Log".into()])),
                    help_area,
                );
                return;
            }

            if self.display_help {
                if let Some(top) = self.views.top_mut() {
                    frame.render_stateful_widget(
                        HelpView::new(top.help_items()),
                        hcontent,
                        &mut self.help_view_state,
                    );
                }
                frame.render_widget(
                    Text::from(Line::from(vec!["F1 ".bold(), "Close Help".into()])),
                    help_area,
                );
                return;
            }

            if let Some(error_dialog) = self.error.as_ref() {
                error_dialog.draw(frame);
                return;
            }

            if let Some(top) = self.views.top_mut() {
                top.draw(&self.ctx, frame, hcontent);
            } else {
                frame.render_widget(
                    Paragraph::new("No views on stack").white().on_magenta(),
                    hcontent,
                );
            }
            frame.render_widget(
                Text::from(Line::from(vec![
                    "Ctrl+Alt+Q ".bold(),
                    "Quit".into(),
                    "|".into(),
                    "F1 ".bold(),
                    "Help".into(),
                    "|".into(),
                    "F2 ".bold(),
                    "Log".into(),
                ])),
                help_area,
            )
        })?;
        Ok(())
    }

    fn poll_events(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if event::poll(std::time::Duration::from_millis(16))? {
            let event = event::read()?;

            if let event::Event::Key(key) = &event {
                if key.kind == KeyEventKind::Press
                    && key.code == KeyCode::Char('q')
                    && key.modifiers == KeyModifiers::ALT | KeyModifiers::CONTROL
                {
                    self.quit();
                }

                if key.kind == KeyEventKind::Press && key.code == KeyCode::F(2) {
                    self.display_log = !self.display_log;
                    return Ok(());
                }

                if self.display_log && key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char(' ') => {
                            self.tui_logger_state.transition(TuiWidgetEvent::SpaceKey)
                        }
                        KeyCode::Esc => self.tui_logger_state.transition(TuiWidgetEvent::EscapeKey),
                        KeyCode::PageUp => self
                            .tui_logger_state
                            .transition(TuiWidgetEvent::PrevPageKey),
                        KeyCode::PageDown => self
                            .tui_logger_state
                            .transition(TuiWidgetEvent::NextPageKey),
                        KeyCode::Up => self.tui_logger_state.transition(TuiWidgetEvent::UpKey),
                        KeyCode::Down => self.tui_logger_state.transition(TuiWidgetEvent::DownKey),
                        KeyCode::Left => self.tui_logger_state.transition(TuiWidgetEvent::LeftKey),
                        KeyCode::Right => {
                            self.tui_logger_state.transition(TuiWidgetEvent::RightKey)
                        }
                        KeyCode::Char('+') => {
                            self.tui_logger_state.transition(TuiWidgetEvent::PlusKey)
                        }
                        KeyCode::Char('-') => {
                            self.tui_logger_state.transition(TuiWidgetEvent::MinusKey)
                        }
                        KeyCode::Char('h') => {
                            self.tui_logger_state.transition(TuiWidgetEvent::HideKey)
                        }
                        KeyCode::Char('f') => {
                            self.tui_logger_state.transition(TuiWidgetEvent::FocusKey)
                        }
                        _ => {}
                    }
                    return Ok(());
                }

                if key.kind == KeyEventKind::Press && key.code == KeyCode::F(1) {
                    self.display_help = !self.display_help;
                    return Ok(());
                }

                if self.error.is_some() {
                    if key.kind == KeyEventKind::Press && key.code == KeyCode::Enter {
                        self.error = None;
                    }
                    return Ok(());
                }
            }

            if let Some(top) = self.views.top_mut() {
                top.on_input(&mut self.ctx, &event);
            }
        }
        Ok(())
    }
}
