use proton_api_core::services::proton::response_data::HumanVerificationChallenge;

/// Specific Reason for error occurrence
///
/// This types aggregates all the possible reasons for an error to occur in the mail module.
#[derive(Debug)]
pub enum MailErrorReason {
    ActionReason(ActionErrorReason),
    SessionReason(ContextErrorReason),
    LoginReason(LoginErrorReason),
    DraftReason(DraftErrorReason),
    EventReason(EventErrorReason),
    OtherReason(OtherErrorReason),
}

impl From<ActionErrorReason> for MailErrorReason {
    fn from(reason: ActionErrorReason) -> Self {
        Self::ActionReason(reason)
    }
}

impl From<ContextErrorReason> for MailErrorReason {
    fn from(reason: ContextErrorReason) -> Self {
        Self::SessionReason(reason)
    }
}

impl From<LoginErrorReason> for MailErrorReason {
    fn from(reason: LoginErrorReason) -> Self {
        Self::LoginReason(reason)
    }
}

impl From<DraftErrorReason> for MailErrorReason {
    fn from(reason: DraftErrorReason) -> Self {
        Self::DraftReason(reason)
    }
}

impl From<EventErrorReason> for MailErrorReason {
    fn from(reason: EventErrorReason) -> Self {
        Self::EventReason(reason)
    }
}

impl From<OtherErrorReason> for MailErrorReason {
    fn from(reason: OtherErrorReason) -> Self {
        Self::OtherReason(reason)
    }
}

/// Specific Reason for error occurrence within ActionQueue
///
/// This enum is used to represent the specific reason for an error that occurred
/// in oreder to provide only the necessary information to the user.
#[derive(Debug)]
pub enum ActionErrorReason {
    UnknownLabel,
    UnknownMessage,
}

/// Specific Reason for error occurrence within Context.
///
/// This enum is used to represent the specific reason for an error that occurred
/// in handling context related operations in order to provide only the necessary
/// information to the user. This error type in uniffi library is named `SessionErrorReason`
/// as the session is nomeclature used in the client library.
#[derive(Debug)]
pub enum ContextErrorReason {
    UnknownLabel,
}

/// Specific Reason for error occurrence within Login Flow.
///
/// This enum is used to represent the specific reason for an error that occurred
/// in handling login related operations in order to provide only the necessary
/// information to the user.
#[derive(Debug)]
pub enum LoginErrorReason {
    HumanVerificationChallenge(HumanVerificationChallenge),
    InvalidCredentials,
    UnsupportedTfa,
    CantUnlockUserKey,
}

/// Specific Reason for error occurrence within Draft.
///
/// This enum is used to represent the specific reason for an error that occurred
/// while drafting a new message in order to provide only the necessary
/// information to the user.
#[derive(Debug)]
pub enum DraftErrorReason {
    UnknownMimeType,
}

/// Specific Reason for error occurrence within Event Loop.
///
/// This enum is used to represent the specific reason for an error that occurred
/// in handling event loop related operations in order to provide only the necessary
/// information to the user.
#[derive(Debug)]
pub enum EventErrorReason {
    Placeholder,
}

/// Specific Reason for error occurrence within the application.
///
/// This enum is used to represent the specific reason for an error that occurred
/// in handling application related operations in order to provide a way to descirbe
/// common reasons across the application execution errors.
#[derive(Debug)]
pub enum OtherErrorReason {
    InvalidParameter,
    Other(String),
}
