use crate::queue::{DispatchQueue, LocalDispatchObject, QueueDispatcher};
use crate::view::{View, ViewStack};
use crate::views::ErrorDialog;
use crate::TerminalType;
use crossterm::event;
use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Flex, Layout};
use ratatui::prelude::Stylize;
use ratatui::widgets::{Block, Borders, Paragraph};

pub struct AppContext<T: 'static, E: 'static + Send> {
    state: T,
    dispatch_queue: DispatchQueue<App<T, E>>,
    local_queue: Vec<LocalDispatchObject<App<T, E>>>,
}

impl<T: 'static, E: 'static + Send> AppContext<T, E> {
    pub fn set_error<Error: std::error::Error + Send + 'static, S: Into<String>>(
        &self,
        desc: S,
        err: Error,
    ) {
        let desc = desc.into();
        self.dispatch_queue.dispatcher().queue_sync(move |app| {
            app.set_error(desc, err);
        });
    }

    #[allow(unused)]
    pub fn quit(&self) {
        self.dispatch_queue.dispatcher().queue_sync(|app| {
            app.quit();
        });
    }

    pub fn dispatcher(&self) -> AppDispatcher<T, E> {
        AppDispatcher::new(self.dispatch_queue.dispatcher().clone())
    }

    pub fn queue_on_main(&mut self, f: impl FnOnce(&mut App<T, E>) + 'static) {
        self.local_queue.push(Box::new(f));
    }

    pub fn state(&self) -> &T {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut T {
        &mut self.state
    }

    #[allow(unused)]
    pub fn queue_event(&self, e: E) {
        self.dispatch_queue
            .dispatcher()
            .queue_sync(|app| app.events.push(e))
    }

    pub fn push_view<V: View<Self, E> + 'static>(&mut self, view: V) {
        self.queue_on_main(|app| app.push_view(view));
    }

    pub fn pop_and_push_view<V: View<Self, E> + 'static>(&mut self, view: V) {
        self.queue_on_main(|app| app.pop_and_push_view(view));
    }

    pub fn pop_view(&mut self) {
        self.queue_on_main(|app| app.pop_view());
    }
}

#[derive(Clone)]
pub struct AppDispatcher<T: 'static, E: 'static + Send>(QueueDispatcher<App<T, E>>);

impl<T: 'static, E: 'static + Send> AppDispatcher<T, E> {
    pub fn new(d: QueueDispatcher<App<T, E>>) -> Self {
        Self(d)
    }
    pub fn queue_event(&self, e: E) {
        self.0.queue_sync(|app| app.events.push(e));
    }

    pub async fn queue_event_async(&self, e: E) {
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

pub struct App<T: 'static, E: 'static + Send> {
    ctx: AppContext<T, E>,
    views: ViewStack<AppContext<T, E>, E>,
    events: Vec<E>,
    error: Option<ErrorDialog>,
    quit: bool,
}

impl<T: Sized + 'static, E: Send + 'static> App<T, E> {
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
            quit: false,
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

    pub fn pop_and_push_view<V: View<AppContext<T, E>, E> + 'static>(&mut self, view: V) {
        self.views.pop_and_push_view(view);
    }

    pub fn pop_view(&mut self) {
        self.views.pop_view();
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
            self.views.on_event(&mut self.ctx, event);
        }

        self.views.process(&mut self.ctx);
    }

    fn draw(&mut self, terminal: &mut TerminalType) -> Result<(), Box<dyn std::error::Error>> {
        terminal.draw(|frame| {
            let block = Block::new()
                .borders(Borders::TOP)
                .white()
                .on_magenta()
                .title(" Proton Mail TUI  (Quit: Ctrl+Alt+Q) ");
            frame.render_widget(block, frame.size());

            let [_, content, help] = Layout::vertical([
                Constraint::Length(1),
                Constraint::Min(10),
                Constraint::Length(3),
            ])
            .areas(frame.size());
            let help_block = Block::new().borders(Borders::all()).white();
            frame.render_widget(help_block, help);

            let [_, hcontent, _] = Layout::horizontal([
                Constraint::Length(1),
                Constraint::Min(8),
                Constraint::Length(1),
            ])
            .areas(content);
            let [_, help, _] = Layout::vertical([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .areas(help);
            let [_, title_area, _, help_area, _] = Layout::horizontal([
                Constraint::Length(1),
                Constraint::Max(20),
                Constraint::Length(1),
                Constraint::Min(20),
                Constraint::Length(1),
            ])
            .flex(Flex::Center)
            .areas(help);

            if let Some(error_dialog) = self.error.as_ref() {
                error_dialog.draw(frame);
                return;
            }

            if let Some(top) = self.views.top_mut() {
                top.draw(&self.ctx, frame, hcontent);
                frame.render_widget(
                    Paragraph::new(top.name())
                        .magenta()
                        .on_white()
                        .bold()
                        .centered(),
                    title_area,
                );
                top.draw_help(&self.ctx, frame, help_area);
            } else {
                frame.render_widget(
                    Paragraph::new("No views on stack").white().on_magenta(),
                    hcontent,
                );
            }
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
            }

            if self.error.is_some() {
                if let event::Event::Key(key) = &event {
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
