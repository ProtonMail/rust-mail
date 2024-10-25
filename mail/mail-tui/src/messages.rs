use crate::app_model::{AppState, Popup};
use anyhow::anyhow;
use proton_mail_common::{MailContextError, MailboxError};

/// Application messages.
pub enum Messages {
    Login(crate::app_model::login::Message),
    SessionSelect(crate::app_model::session_select::Message),
    ContextInit(crate::app_model::context_init::Message),
    Mailbox(crate::app_model::mailbox::Message),
    TwoFA(crate::app_model::twofa::Message),
    DisplayError(Option<String>, anyhow::Error),
    /// This message can be used to switch the application state.
    SwitchAppState(AppState),
    /// Display an infinite progress indicator with a given message.
    ///
    /// Be sure to send a [`DismissBackgroundProgress`] message to dismiss the indicator when
    /// finished.
    DisplayBackgroundProgress(String),
    /// Dismiss progress indicator (if any).
    DismissBackgroundProgress,
    /// Raise a popup window.
    RaisePopup(Box<dyn Popup + Send>),
    /// Dismiss active pop (if any).
    DismissPopup,
}

impl Messages {
    /// Utility helper to create raise popup messages.
    pub fn raise_popup(pop_up: impl Popup + Send + 'static) -> Self {
        Self::RaisePopup(Box::new(pop_up))
    }
}

impl From<MailContextError> for Messages {
    fn from(value: MailContextError) -> Self {
        Self::DisplayError(None, anyhow!("{value}"))
    }
}

impl From<MailboxError> for Messages {
    fn from(value: MailboxError) -> Self {
        Self::DisplayError(None, anyhow!("{value}"))
    }
}

impl From<anyhow::Error> for Messages {
    fn from(value: anyhow::Error) -> Self {
        Self::DisplayError(None, value)
    }
}
