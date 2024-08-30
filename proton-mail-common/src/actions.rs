mod available_action;
pub mod conversations;
pub mod labels;
pub mod messages;

pub use self::available_action::*;
use crate::AppError;
use proton_action_queue::action::Factory;
use proton_api_core::service::ApiServiceError;
use stash::stash::StashError;

#[derive(Debug, thiserror::Error)]
pub enum ActionError {
    #[error("Http: {0}")]
    Http(#[from] ApiServiceError),
    #[error("Stash: {0}")]
    Stash(#[from] StashError),
    #[error("App: {0}")]
    App(#[from] AppError),
    #[error("No input provided")]
    NoInput,
    #[error("Other: {0}")]
    Other(anyhow::Error),
}

impl proton_action_queue::action::Error for ActionError {
    fn request_error(&self) -> Option<&ApiServiceError> {
        match self {
            Self::Http(e) => Some(e),
            _ => None,
        }
    }
}

pub(crate) fn new_action_factory() -> Factory {
    let mut factory = Factory::new();
    const ERR_MSG: &str = "Double Factory registration";
    factory.register::<conversations::Delete>().expect(ERR_MSG);
    factory.register::<conversations::Unlabel>().expect(ERR_MSG);
    factory.register::<conversations::Label>().expect(ERR_MSG);
    factory
        .register::<conversations::MarkRead>()
        .expect(ERR_MSG);
    factory
        .register::<conversations::MarkUnread>()
        .expect(ERR_MSG);
    factory.register::<conversations::Move>().expect(ERR_MSG);
    factory.register::<messages::label::Label>().expect(ERR_MSG);
    factory
}
