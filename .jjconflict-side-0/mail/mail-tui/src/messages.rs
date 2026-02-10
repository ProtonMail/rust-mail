use crate::app::Command;
use crate::app_model::path_select_popup::PathSelectClosure;
use crate::app_model::{AppState, Popup};
use anyhow::anyhow;
use proton_mail_api::proton_core_api::services::proton::UserId;
use proton_mail_common::MailContextError;
use proton_network_monitor_service::{OsNetworkStatus, RequestNetworkStatus};
use std::path::Path;

/// Application messages.
pub enum Messages {
    Login(crate::app_model::login::Message),
    SessionSelect(crate::app_model::session_select::Message),
    Contacts(crate::app_model::contacts::Message),
    ContextInit(crate::app_model::context_init::Message),
    Mailbox(crate::app_model::mailbox::Message),
    TwoFA(crate::app_model::twofa::Message),
    DisplayError(Option<String>, anyhow::Error),
    DisplayInfo(Option<String>, String),
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
    RaisePopup(Box<dyn Popup + Send + 'static>),
    /// Dismiss active pop (if any).
    DismissPopup,
    /// Raise a dialog to select file paths.
    SelectFilePathPopup(PathSelectClosure),
    /// Background worker messages
    BackgroundWorker(crate::app_model::background::Message),
    SessionExpired(UserId),
    OsNetworkStatusUpdate(OsNetworkStatus),
    RequestNetworkStatusUpdate(RequestNetworkStatus),
}

impl Messages {
    /// Utility helper to create raise popup messages.
    pub fn raise_popup(pop_up: impl Popup + Send + 'static) -> Self {
        Self::RaisePopup(Box::new(pop_up))
    }

    pub fn select_file_path(
        on_select: impl Fn(&Path) -> Command<Messages> + Send + 'static,
    ) -> Self {
        Self::SelectFilePathPopup(Box::new(on_select))
    }
}

impl From<MailContextError> for Messages {
    fn from(value: MailContextError) -> Self {
        Self::DisplayError(None, anyhow!("{value}"))
    }
}

impl From<anyhow::Error> for Messages {
    fn from(value: anyhow::Error) -> Self {
        Self::DisplayError(None, value)
    }
}
