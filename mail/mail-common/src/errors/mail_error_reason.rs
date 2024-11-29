use proton_api_core::services::proton::response_data::HumanVerificationChallenge;

/// Specific Reason for error occurrence
#[derive(Debug)]
pub enum MailErrorReason {
    ActionReason(ActionErrorReason),
    SessionReason(SessionErrorReason),
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

impl From<SessionErrorReason> for MailErrorReason {
    fn from(reason: SessionErrorReason) -> Self {
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

#[derive(Debug)]
pub enum ActionErrorReason {
    UnknownLabel,
    UnknownMessage,
}

#[derive(Debug)]
pub enum SessionErrorReason {
    UnknownLabel,
}

#[derive(Debug)]
pub enum LoginErrorReason {
    HumanVerificationChallenge(HumanVerificationChallenge),
    InvalidCredentials,
    UnsupportedTfa,
    CantUnlockUserKey,
}

#[derive(Debug)]
pub enum DraftErrorReason {
    UnknownMimeType,
}

#[derive(Debug)]
pub enum EventErrorReason {
    Placeholder,
}

#[derive(Debug)]
pub enum OtherErrorReason {
    InvalidParameter,
    Other(String),
}
