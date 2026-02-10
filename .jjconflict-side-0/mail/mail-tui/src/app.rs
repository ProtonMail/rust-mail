use crate::TerminalType;
use crate::app_model::InfoDialog;
use crate::messages::Messages;
use anyhow::bail;
use crossterm::event::{Event, EventStream};
use flume::{Receiver, Sender};
use futures::future::BoxFuture;
use futures::{FutureExt as _, StreamExt as _};
use ratatui::crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
use ratatui::prelude::*;
use std::future::Future;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::select;
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
    fn handle_event(&mut self, event: Event) -> Command<Message>;
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
    quit: bool,
}

impl<M: Model<Message> + Sized, Message: Send + 'static> App<M, Message> {
    pub fn new(model: M) -> Self {
        let (sender, receiver) = flume::unbounded();

        Self {
            model,
            quit: false,
            bg_receiver: receiver,
            bg_sender: sender,
        }
    }

    fn handle_event(
        &mut self,
        event: Option<Result<Event, impl std::error::Error>>,
    ) -> anyhow::Result<Command<Message>> {
        match event {
            Some(Ok(Event::Key(key)))
                if (key.kind == KeyEventKind::Press
                    && key.code == KeyCode::Char('c')
                    && key.modifiers == KeyModifiers::CONTROL) =>
            {
                tracing::info!("Ctrl + C received, exiting...");
                self.quit();
                Ok(Command::None)
            }
            Some(Ok(event)) => Ok(self.model.handle_event(event)),
            Some(Err(e)) => {
                tracing::error!("crossterm error: {e:?}");
                Ok(Command::None)
            }
            None => bail!("crossterm stopped emitting events"),
        }
    }

    pub fn run(&mut self, mut terminal: TerminalType, runtime: &Runtime) -> anyhow::Result<()> {
        // Initialize.
        {
            // handle init.
            let message = self.model.on_ready();
            self.handle_command(message, runtime);
        }

        let mut reader = EventStream::new();

        // We do it like this to avoid being in a tokio context when handling events or commands.
        // We want this to make `tokio::spawn` panic
        // The main idea about having everything sync is to compare the integration of our code
        // with other tech stacks that are not async.
        //
        // This could be simplified if we run this fully async, but we don't want to do that for now.
        #[allow(clippy::items_after_statements)]
        enum DoOutsideAsync<Message> {
            None,
            HandleEvent(Option<Result<Event, std::io::Error>>),
            HandleCommand(Command<Message>),
        }

        while !self.quit {
            terminal.draw(|frame| self.model.view(frame))?;
            let msg = runtime.block_on(async {
                let msg = select! {
                    event = reader.next().fuse() => {
                        DoOutsideAsync::HandleEvent(event)
                    }
                    msg = self.bg_receiver.recv_async() => {
                            DoOutsideAsync::HandleCommand(msg?)
                        }

                    // This is here to make sure the animations like throbbers can progress if there no inputs or actions.
                    () = tokio::time::sleep(Duration::from_millis(250)) => {
                        DoOutsideAsync::None
                    }
                };
                Ok::<_, anyhow::Error>(msg)
            })?;

            match msg {
                DoOutsideAsync::None => (),
                DoOutsideAsync::HandleEvent(event) => {
                    let command = self.handle_event(event)?;
                    self.handle_command(command, runtime);
                }
                DoOutsideAsync::HandleCommand(command) => {
                    self.handle_command(command, runtime);
                }
            }
        }
        Ok(())
    }

    /// Terminate the application.
    pub fn quit(&mut self) {
        self.quit = true;
    }

    fn handle_command(&mut self, command: Command<Message>, runtime: &Runtime) {
        if command.is_none() {
            // skip allocation just below
            return;
        }

        let mut pending = Vec::with_capacity(4);
        pending.push(command);
        while let Some(command) = pending.pop() {
            match command {
                Command::None => (),
                Command::Message(message) => {
                    pending.push(self.model.update(message));
                }
                Command::Task(future) => {
                    let sender = self.bg_sender.clone();
                    runtime.spawn(async move {
                        let command = future.await;
                        if sender.send_async(command).await.is_err() {
                            error!("Failed to send background command");
                        }
                    });
                }
                Command::BackgroundTask(closure) => {
                    let sender = self.bg_sender.clone();
                    runtime.spawn(closure(sender));
                }
                Command::Batch(commands) => pending.extend(commands.into_iter().rev()),
            }
        }
    }
}

/// Execute an action in the application.
#[derive(Default)]
pub enum Command<Message> {
    #[default]
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
        Self::command_from_future(async move { f.await.map(|()| Command::None) })
    }

    pub fn command_from_future(
        f: impl Future<Output = anyhow::Result<Command<Messages>>> + Send + 'static,
    ) -> Self {
        Command::task(async move {
            match f.await {
                Ok(out) => out,
                Err(e) => {
                    tracing::error!("{e:?}");
                    Command::message(e)
                }
            }
        })
    }

    pub fn popup_from_future(
        title: impl Into<String>,
        f: impl Future<Output = anyhow::Result<String>> + Send + 'static,
    ) -> Self {
        let title = title.into();
        Command::task(async move {
            match f.await {
                Ok(s) => Messages::raise_popup(InfoDialog::new_info(Some(title), s)).into(),
                Err(e) => {
                    tracing::error!("{e:?}");
                    Messages::raise_popup(InfoDialog::new_error(Some(title), e)).into()
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
    pub fn message(message: impl Into<Message>) -> Self {
        Self::Message(message.into())
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
