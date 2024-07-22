use crate::TerminalType;
use crossterm::event;
use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
use proton_async::runtime::MultiThreaded;
use proton_async::sync::mpsc::{unbounded, Receiver, Sender};
use proton_mail_common::exports::tracing::error;
use ratatui::prelude::*;
use std::future::Future;
use std::pin::Pin;

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
    /// If you want to run an async tasks in the background return [`Command::task`]. The `sender`
    /// is passed here for situations where you are running something on a dedicated thread and
    /// need to communicate back to the main thread.
    fn update(&mut self, message: Message, sender: &Sender<Command<Message>>) -> Command<Message>;
    /// Called to display the appication.
    fn view(&mut self, frame: &mut Frame);

    fn runtime(&self) -> &MultiThreaded;
}
pub struct App<M: Model<Message>, Message: Send + 'static> {
    model: M,
    bg_receiver: Receiver<Command<Message>>,
    bg_sender: Sender<Command<Message>>,
    quit: bool,
}

impl<M: Model<Message> + Sized, Message: Send + 'static> App<M, Message> {
    pub fn new(model: M) -> Self {
        let (sender, receiver) = unbounded();
        Self {
            model,
            quit: false,
            bg_receiver: receiver,
            bg_sender: sender,
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
                    pending.push(self.model.update(message, &self.bg_sender));
                }
                Command::Task(future) => {
                    let sender = self.bg_sender.clone();
                    self.model.runtime().spawn(async move {
                        let command = future.await;
                        if sender.send(command).is_err() {
                            error!("Failed to send background command");
                        }
                    });
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
    Task(Pin<Box<dyn Future<Output = Command<Message>> + Send + 'static>>),
    Batch(Vec<Command<Message>>),
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

    /// This command runs the supplied `commands` in order.
    pub fn batch(commands: impl IntoIterator<Item = Command<Message>>) -> Self {
        Self::Batch(Vec::from_iter(commands))
    }

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
