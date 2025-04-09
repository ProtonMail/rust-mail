use crate::TerminalType;
use crate::messages::Messages;
use flume::{Receiver, Sender};
use futures::future::BoxFuture;
use ratatui::crossterm::event;
use ratatui::crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
use ratatui::prelude::*;
use std::future::Future;
use tokio::runtime::Runtime;
use tracing::error;

/// Behavior specification for the model.
pub trait Model<Message> {
    /// Called when the application is about to enter the main loop.
    ///
    /// If a `Message` is returned, [`update`] will be called until no more messages are returned.
    fn on_ready(&mut self) -> Command<Message>;
    /// Called when there is an event.
    ///
    /// This method is called once per tick.
    fn handle_event(&mut self, event: event::Event) -> Command<Message>;
    /// Called when a message has been received.
    ///
    /// If a [`Command`] is returned, [`update`] will be called until no more messages are returned.
    ///
    /// If you want to run an async tasks in the background return [`Command::task`].
    fn update(&mut self, message: Message) -> Command<Message>;
    /// Called to display the appication.
    fn view(&mut self, frame: &mut Frame);
}
pub struct App<M: Model<Message>, Message: Send + 'static> {
    model: M,
    bg_receiver: Receiver<Command<Message>>,
    bg_sender: Sender<Command<Message>>,
    runtime: Runtime,
    quit: bool,
}

impl<M: Model<Message> + Sized, Message: Send + 'static> App<M, Message> {
    pub fn new(runtime: Runtime, model: M) -> Self {
        let (sender, receiver) = flume::unbounded();

        Self {
            model,
            quit: false,
            bg_receiver: receiver,
            bg_sender: sender,
            runtime,
        }
    }

    pub fn run(&mut self, mut terminal: TerminalType) -> Result<(), Box<dyn std::error::Error>> {
        // Initialize.
        {
            // handle init.
            let message = self.model.on_ready();
            self.handle_command(message);
        }

        while !self.quit {
            // draw frame.
            terminal.draw(|frame| self.model.view(frame))?;

            // Handle background issued messages.
            while let Ok(message) = self.bg_receiver.try_recv() {
                self.handle_command(message);
            }

            // handle input
            let message = self.poll_events()?;

            // Apply updates from input.
            self.handle_command(message);
        }

        Ok(())
    }

    /// Terminate the application.
    pub fn quit(&mut self) {
        self.quit = true;
    }

    fn poll_events(&mut self) -> Result<Command<Message>, Box<dyn std::error::Error>> {
        if event::poll(std::time::Duration::from_millis(250))? {
            let event = event::read()?;

            if let event::Event::Key(key) = &event {
                if key.kind == KeyEventKind::Press
                    && key.code == KeyCode::Char('c')
                    && key.modifiers == KeyModifiers::CONTROL
                {
                    self.quit();
                }
            }

            return Ok(self.model.handle_event(event));
        }
        Ok(Command::None)
    }

    fn handle_command(&mut self, command: Command<Message>) {
        let mut pending = Vec::with_capacity(4);
        pending.push(command);
        while let Some(command) = pending.pop() {
            match command {
                Command::None => {}
                Command::Message(message) => {
                    pending.push(self.model.update(message));
                }
                Command::Task(future) => {
                    let sender = self.bg_sender.clone();
                    self.runtime.spawn(async move {
                        let command = future.await;
                        if sender.send_async(command).await.is_err() {
                            error!("Failed to send background command");
                        }
                    });
                }
                Command::BackgroundTask(closure) => {
                    let sender = self.bg_sender.clone();
                    self.runtime.spawn(closure(sender));
                }
                Command::Batch(commands) => pending.extend(commands.into_iter().rev()),
            }
        }
    }
}

/// Execute an action in the application.
pub enum Command<Message> {
    None,
    Message(Message),
    Task(BoxFuture<'static, Command<Message>>),
    Batch(Vec<Command<Message>>),
    BackgroundTask(
        Box<dyn FnOnce(Sender<Command<Message>>) -> BoxFuture<'static, ()> + Send + 'static>,
    ),
}

impl Command<Messages> {
    pub fn from_future(f: impl Future<Output = anyhow::Result<()>> + Send + 'static) -> Self {
        Command::task(async move {
            match f.await {
                Ok(()) => Command::None,
                Err(e) => {
                    tracing::error!("{e:?}");
                    Command::message(e.into())
                }
            }
        })
    }
}

impl<Message> Command<Message> {
    /// This command does nothing.
    pub fn none() -> Self {
        Self::None
    }

    /// This command sends the `message` to the model.
    pub fn message(message: Message) -> Self {
        Self::Message(message)
    }

    /// This command spawns `task` in a new async task and then sends the result back to
    /// the main application.
    pub fn task(task: impl Future<Output = Command<Message>> + Send + 'static) -> Self {
        Self::Task(Box::pin(task))
    }

    /// Create a new background task that can produce messages.
    pub fn background_task(
        task: impl FnOnce(Sender<Command<Message>>) -> BoxFuture<'static, ()> + Send + 'static,
    ) -> Self {
        Self::BackgroundTask(Box::new(task))
    }

    /// This command runs the supplied `commands` in order.
    pub fn batch(commands: impl IntoIterator<Item = Command<Message>>) -> Self {
        Self::Batch(Vec::from_iter(commands))
    }

    #[allow(dead_code)]
    pub fn is_some(&self) -> bool {
        !matches!(self, Command::None)
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Command::None)
    }
}

impl<Message> Default for Command<Message> {
    fn default() -> Self {
        Self::None
    }
}
