use proton_core_api::services::proton::AddressId;

/// Specific Reason for error occurrence
///
/// This types aggregates all the possible reasons for an error to occur in the mail module.
#[derive(Debug)]
pub enum MailErrorReason {
    ActionReason(ActionErrorReason),
    SessionReason(ContextErrorReason),
    LoginReason(LoginErrorReason),
    SignupReason(SignupErrorReason),
    DraftOpenReason(DraftOpenErrorReason),
    DraftSaveReason(DraftSaveErrorReason),
    DraftSendReason(DraftSendErrorReason),
    DraftUndoSendReason(DraftUndoSendErrorReason),
    DraftDiscardReason(DraftDiscardErrorReason),
    DraftAttachmentUploadReason(DraftAttachmentUploadErrorReason),
    DraftAttachmentRemoveReason(DraftAttachmentRemoveErrorReason),
    DraftCancelScheduleSendReason(DraftCancelScheduleSendErrorReason),
    EventReason(EventErrorReason),
    PinSetReson(PinSetErrorReason),
    PinAuthReson(PinAuthErrorReason),
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

impl From<SignupErrorReason> for MailErrorReason {
    fn from(reason: SignupErrorReason) -> Self {
        Self::SignupReason(reason)
    }
}

impl From<DraftOpenErrorReason> for MailErrorReason {
    fn from(reason: DraftOpenErrorReason) -> Self {
        Self::DraftOpenReason(reason)
    }
}

impl From<DraftSendErrorReason> for MailErrorReason {
    fn from(value: DraftSendErrorReason) -> Self {
        Self::DraftSendReason(value)
    }
}

impl From<DraftSaveErrorReason> for MailErrorReason {
    fn from(value: DraftSaveErrorReason) -> Self {
        Self::DraftSaveReason(value)
    }
}

impl From<DraftUndoSendErrorReason> for MailErrorReason {
    fn from(value: DraftUndoSendErrorReason) -> Self {
        Self::DraftUndoSendReason(value)
    }
}

impl From<PinSetErrorReason> for MailErrorReason {
    fn from(reason: PinSetErrorReason) -> Self {
        Self::PinSetReson(reason)
    }
}

impl From<PinAuthErrorReason> for MailErrorReason {
    fn from(reason: PinAuthErrorReason) -> Self {
        Self::PinAuthReson(reason)
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
/// in order to provide only the necessary information to the user.
#[derive(Debug)]
pub enum ActionErrorReason {
    UnknownLabel,
    UnknownMessage,
    UnknownContentId,
}

/// Specific Reason for error occurrence within Context.
///
/// This enum is used to represent the specific reason for an error that occurred
/// in handling context related operations in order to provide only the necessary
/// information to the user. This error type in uniffi library is named `SessionErrorReason`
/// as the session is nomenclature used in the client library.
#[derive(Debug)]
pub enum ContextErrorReason {
    UnknownLabel,
    DuplicateContext,
    UserContextNotInitialized(String),
}

/// Specific Reason for error occurrence within Login Flow.
///
/// This enum is used to represent the specific reason for an error that occurred
/// in handling login related operations in order to provide only the necessary
/// information to the user.
#[derive(Debug)]
pub enum LoginErrorReason {
    InvalidCredentials,
    UnsupportedTfa,
    CantUnlockUserKey,
}

/// Specific Reason for error occurrence within Signup Flow.
///
/// This enum is used to represent the specific reason for an error that occurred
/// in handling signup related operations in order to provide only the necessary
/// information to the user.
#[derive(Debug)]
pub enum SignupErrorReason {
    SignupBlockedByServer,
    UsernameUnavailable,
    AccountCreationFailed,
    AddressSetupFailed,
    KeySetupFailed,
}

/// Specific Reason when opening a draft fails.
///
/// This enum is used to represent the specific reason for an error that occurred
/// while drafting a new message in order to provide only the necessary
/// information to the user.
#[derive(Debug)]
pub enum DraftOpenErrorReason {
    /// This message no longer exists.
    MessageDoesNotExist,
    /// This message is not a draft
    MessageIsNotADraft,
    /// Attempting to reply or forward to a draft
    ReplyOrForwardDraft,
    /// Could not find the user's address
    AddressNotFound,
    /// Message body is missing
    MessageBodyMissing,
}

/// Specific Reason when saving a draft
#[derive(Debug)]
pub enum DraftSaveErrorReason {
    /// Address does not have a primary key
    AddressDoesNotHavePrimaryKey(AddressId),
    /// Recipient email is invalid
    RecipientEmailInvalid(String),
    /// This Proton recipient does not exist.
    ProtonRecipientDoesNotExist(String),
    /// Some other validation error occurred for this recipient
    UnknownRecipientValidationError(String),
    /// This address is disabled and can't be used for sending
    AddressDisabled(String),
    /// Message was already sent.
    MessageAlreadySent,
    /// This draft was already sent and can't be modified
    AlreadySent,
    /// This message no longer exists.
    MessageDoesNotExist,
    /// Message is not a draft
    MessageIsNotADraft,
}

/// Specific Reason when saving a draft
#[derive(Debug)]
pub enum DraftSendErrorReason {
    /// Message has no recipients
    NoRecipients,
    /// Recipient email is invalid
    RecipientEmailInvalid(String),
    /// This Proton recipient does not exist.
    ProtonRecipientDoesNotExist(String),
    /// Some other validation error occurred for this recipient
    UnknownRecipientValidationError(String),
    /// A packaging error occurred
    PackageError(String),
    /// This message no longer exists.
    MessageDoesNotExist,
    /// Message is not a draft
    MessageIsNotADraft,
    /// Message is missing attachment uploads
    MissingAttachmentUploads,
    /// Time at which the message was scheduled to send has already expired
    ScheduleSendExpired,
}

/// Specific Reason when attempting to cancel sending of an already sent draft.
///
/// This enum is used to represent the specific reason for an error that occurred
/// while saving or sending a draft in order to provide only the necessary
/// information to the user.
#[derive(Debug)]
pub enum DraftUndoSendErrorReason {
    /// Can not undo sent this message
    MessageCanNotBeUndoSent,
    /// The cancellation of sending for this message is no longer possible.
    SendCanNoLongerBeUndone,
    /// Message is not a draft
    MessageIsNotADraft,
    /// This message no longer exists.
    MessageDoesNotExist,
}

/// Failure cases for draft attachment errors.
#[derive(Debug)]
pub enum DraftAttachmentUploadErrorReason {
    /// This message no longer exists.
    MessageDoesNotExist,
    /// Message does not exist on the server
    MessageDoesNotExistOnServer,
    /// Failed to encrypt the attachment
    Crypto,
    /// Too Many Attachments
    TooManyAttachments,
    /// Message was already sent.
    MessageAlreadySent,
    /// the attachment size exceeds the upper limit
    AttachmentTooLarge,
    /// Upload Retry in invalid state
    RetryInvalidState,
}

#[derive(Debug)]
pub enum DraftAttachmentRemoveErrorReason {
    /// Can't remove public key attachments when mail settings to attach public keys are active
    AttachmentIsPublicKey,
}

/// Specific Reason when attempting to discard a draft.
///
/// This enum is used to represent the specific reason for an error that occurred
/// while saving or sending a draft in order to provide only the necessary
/// information to the user.
#[derive(Debug)]
pub enum DraftDiscardErrorReason {
    /// This message does not exist
    MessageDoesNotExist,
    /// Deleting the draft failed
    DeleteFailed,
}

#[derive(Debug)]
pub enum DraftCancelScheduleSendErrorReason {
    MessageDoesNotExist,
    MessageNotScheduled,
    MessageAlreadySent,
}

/// Specific Reason for error occurrence within Event Loop.
///
/// This enum is used to represent the specific reason for an error that occurred
/// in handling event loop related operations in order to provide only the necessary
/// information to the user.
#[derive(Debug)]
pub enum EventErrorReason {
    Refresh,
    Subscriber,
}

/// Specific Reason for error occurrence while creating user's PIN
///
#[derive(Debug)]
pub enum PinSetErrorReason {
    TooShort,
    TooLong,
    Malformed,
}

/// Specific Reason for error occurrence while authenticating user with PIN
///
#[derive(Debug)]
pub enum PinAuthErrorReason {
    TooManyAttempts,
    TooFrequentAttempts,
    IncorrectPin,
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
