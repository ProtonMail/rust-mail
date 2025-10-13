use proton_core_api::services::proton::{AddressId, PrivateEmail};

/// Specific Reason for error occurrence
///
/// This types aggregates all the possible reasons for an error to occur in the mail module.
#[derive(Debug)]
pub enum MailErrorReason {
    ActionReason(ActionErrorReason),
    ContextReason(ContextErrorReason),
    DraftOpenReason(DraftOpenErrorReason),
    DraftSaveReason(DraftSaveErrorReason),
    DraftSendReason(DraftSendErrorReason),
    DraftUndoSendReason(DraftUndoSendErrorReason),
    DraftDiscardReason(DraftDiscardErrorReason),
    DraftAttachmentUploadReason(DraftAttachmentUploadErrorReason),
    DraftCancelScheduleSendReason(DraftCancelScheduleSendErrorReason),
    DraftSenderAddressChangeReason(DraftSenderAddressChangeErrorReason),
    DraftPasswordReason(DraftPasswordErrorReason),
    DraftExpirationReason(DraftExpirationErrorReason),
    DraftAttachmentDispositionSwapError(DraftAttachmentDispositionSwapErrorReason),
    EventReason(EventErrorReason),
    PinSetReason(PinSetErrorReason),
    PinAuthReason(PinAuthErrorReason),
    MailScrollerReason(MailScrollerErrorReason),
    SnoozeReason(SnoozeErrorReason),
    OtherReason(OtherErrorReason),
}

impl From<ActionErrorReason> for MailErrorReason {
    fn from(reason: ActionErrorReason) -> Self {
        Self::ActionReason(reason)
    }
}

impl From<ContextErrorReason> for MailErrorReason {
    fn from(reason: ContextErrorReason) -> Self {
        Self::ContextReason(reason)
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
        Self::PinSetReason(reason)
    }
}

impl From<PinAuthErrorReason> for MailErrorReason {
    fn from(reason: PinAuthErrorReason) -> Self {
        Self::PinAuthReason(reason)
    }
}

impl From<MailScrollerErrorReason> for MailErrorReason {
    fn from(reason: MailScrollerErrorReason) -> Self {
        Self::MailScrollerReason(reason)
    }
}

impl From<SnoozeErrorReason> for MailErrorReason {
    fn from(reason: SnoozeErrorReason) -> Self {
        Self::SnoozeReason(reason)
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
    DuplicateContext,
    UserContextNotInitialized(String),
}

/// Specific Reason when opening a draft fails.
///
/// This enum is used to represent the specific reason for an error that occurred
/// while drafting a new message in order to provide only the necessary
/// information to the user.
#[derive(Debug)]
pub enum DraftOpenErrorReason {
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
    /// This address is disabled and can't be used for sending
    AddressDisabled(String),
    /// Message was already sent.
    MessageAlreadySent,
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
    RecipientEmailInvalid(PrivateEmail),
    /// This Proton recipient does not exist.
    ProtonRecipientDoesNotExist(PrivateEmail),
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
    /// The maximum number of scheduled send messages has been reached.
    ScheduleSendMessageLimitExceeded,
    /// Failed to decrypt external encryption password
    EOPasswordDecrypt,
    /// Failed to decrypt external encryption password
    ExpirationTimeTooSoon,
    /// Message + Attachments size exceeds limit
    MessageTooLarge,
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
    /// The attachment size exceeds the upper limit
    AttachmentTooLarge,
    /// The combined attachment size exceeds the upper limit
    TotalAttachmentSizeTooLarge,
    /// Upload Retry in invalid state
    RetryInvalidState,
    /// Attachment upload timed out
    Timeout,
    StorageQuotaExceeded,
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

#[derive(Debug)]
pub enum DraftSenderAddressChangeErrorReason {
    AddressNotSendEnabled,
    AddressDisabled,
    AddressWithEmailNotFound(String),
}

#[derive(Debug)]
pub enum DraftPasswordErrorReason {
    PasswordTooShort,
}

#[derive(Debug)]
pub enum DraftExpirationErrorReason {
    ExpirationTimeInThePast,
    ExpirationTimeLessThan15Min,
    ExpirationTimeExceeds28Days,
}

#[derive(Debug)]
pub enum DraftAttachmentDispositionSwapErrorReason {
    InvalidState,
    Noop,
    AttachmentDoesNotExist,
    AttachmentMessageDoesNotExist,
    AttachmentMessageIsNotADraft,
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

/// Specific Reason for error occurrence within Mail Scroller.
///
/// This enum is used to represent the specific reason for an error that occurred
/// in handling mail scroller related operations in order to provide only the necessary
/// information to the user.
#[derive(Debug)]
pub enum MailScrollerErrorReason {
    NotSynced,
}

#[derive(Debug)]
pub enum SnoozeErrorReason {
    SnoozeTimeInThePast,
    InvalidSnoozeLocation,
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
