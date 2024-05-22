use crate::TerminalType;
use crossterm::event;
use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
use proton_async::sync::mpsc::{unbounded, Receiver, Sender};
use ratatui::prelude::*;

/// Behavior specification for the model.
pub trait Model<Message> {
    /// Called when the application is about to enter the main loop.
    ///
    /// If a `Message` is returned, [`update`] will be called until no more messages are returned.
    fn on_ready(&mut self) -> Option<Message>;
    /// Called when there is an event.
    ///
    /// This method is called once per tick.
    fn handle_event(&mut self, event: event::Event) -> Option<Message>;
    /// Called when a message has been received.
    ///
    /// If a `Message` is returned, [`update`] will be called until no more messages are returned.
    ///
    /// To send a message from a background thread, clone the provided `sender`.
    fn update(&mut self, message: Message, sender: &Sender<Message>) -> Option<Message>;
    /// Called to display the appication.
    fn view(&mut self, frame: &mut Frame);
}
pub struct App<M: Model<Message>, Message: Send + 'static> {
    model: M,
    bg_receiver: Receiver<Message>,
    bg_sender: Sender<Message>,
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
            let mut cur_message = self.model.on_ready();

            // Apply updates from the init message.
            while let Some(message) = cur_message {
                cur_message = self.model.update(message, &self.bg_sender);
            }
        }

        while !self.quit {
            // draw frame.
            terminal.draw(|frame| self.model.view(frame))?;

            // Handle background issued messages.
            while let Ok(message) = self.bg_receiver.try_recv() {
                let mut cur_message = self.model.update(message, &self.bg_sender);
                while let Some(message) = cur_message {
                    cur_message = self.model.update(message, &self.bg_sender);
                }
            }

            // handle input
            let mut cur_message = self.poll_events()?;

            // Apply updates from input.
            while let Some(message) = cur_message {
                cur_message = self.model.update(message, &self.bg_sender);
            }
        }

        Ok(())
    }

    /// Terminate the application.
    pub fn quit(&mut self) {
        self.quit = true;
    }

    fn poll_events(&mut self) -> Result<Option<Message>, Box<dyn std::error::Error>> {
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
        Ok(None)
    }
}
