use proton_crypto_inbox::lock_icon::{LockColor, LockIcon, LockTooltip, UiLock};

#[derive(Clone, uniffi::Record)]
pub struct PrivacyLock {
    pub icon: PrivacyLockIcon,
    pub color: PrivacyLockColor,
    pub tooltip: PrivacyLockTooltip,
}

impl From<UiLock> for PrivacyLock {
    fn from(value: UiLock) -> Self {
        Self {
            icon: value.icon.into(),
            color: value.color.into(),
            tooltip: value.tooltip.into(),
        }
    }
}

#[derive(uniffi::Enum, Copy, Clone)]
pub enum PrivacyLockIcon {
    ClosedLock,
    ClosedLockWithTick,
    ClosedLockWithPen,
    ClosedLockWarning,
    OpenLockWithPen,
    OpenLockWithTick,
    OpenLockWarning,
}

impl From<LockIcon> for PrivacyLockIcon {
    fn from(value: LockIcon) -> Self {
        match value {
            LockIcon::ClosedLock => Self::ClosedLock,
            LockIcon::ClosedLockWithTick => Self::ClosedLockWithTick,
            LockIcon::ClosedLockWithPen => Self::ClosedLockWithPen,
            LockIcon::ClosedLockWarning => Self::ClosedLockWarning,
            LockIcon::OpenLockWithPen => Self::OpenLockWithPen,
            LockIcon::OpenLockWithTick => Self::OpenLockWithTick,
            LockIcon::OpenLockWarning => Self::OpenLockWarning,
        }
    }
}

#[derive(uniffi::Enum, Copy, Clone)]
pub enum PrivacyLockColor {
    Black,
    Green,
    Blue,
}

impl From<LockColor> for PrivacyLockColor {
    fn from(value: LockColor) -> Self {
        match value {
            LockColor::Black => Self::Black,
            LockColor::Green => Self::Green,
            LockColor::Blue => Self::Blue,
        }
    }
}

#[derive(uniffi::Enum, Copy, Clone)]
pub enum PrivacyLockTooltip {
    None,
    SendE2E,
    SendE2EVerifiedRecipient,
    SendSignOnly,
    SendZeroAccessEncryptionDisabled,
    ZeroAccess,
    ZeroAccessSentByProton,
    ReceiveE2E,
    ReceiveE2EVerifiedRecipient,
    ReceiveE2EVerificationFailed,
    ReceiveE2EVerificationFailedNoSignature,
    ReceiveSignOnlyVerifiedRecipient,
    ReceiveSignOnlyVerificationFailed,
    SentE2EVerifiedRecipients,
    SentProtonVerifiedRecipients,
    SentE2E,
    SentRecipientE2EVerifiedRecipient,
    SentRecipientProtonMailVerifiedRecipient,
    SentRecipientE2E,
    SentRecipientProtonMail,
    SentRecipientE2EPgpVerifiedRecipient,
    SentRecipientProtonMailPgpVerifiedRecipient,
    SentRecipientE2EPgpRecipient,
    SentRecipientProtonMailPgpRecipient,
    SentRecipientPGPSigned,
}

impl From<LockTooltip> for PrivacyLockTooltip {
    fn from(value: LockTooltip) -> Self {
        match value {
            LockTooltip::None => Self::None,
            LockTooltip::SendE2E => Self::SendE2E,
            LockTooltip::SendE2EVerifiedRecipient => Self::SendE2EVerifiedRecipient,
            LockTooltip::SendSignOnly => Self::SendSignOnly,
            LockTooltip::SendZeroAccessEncryptionDisabled => Self::SendZeroAccessEncryptionDisabled,
            LockTooltip::ZeroAccess => Self::ZeroAccess,
            LockTooltip::ZeroAccessSentByProton => Self::ZeroAccessSentByProton,
            LockTooltip::ReceiveE2E => Self::ReceiveE2E,
            LockTooltip::ReceiveE2EVerifiedRecipient => Self::ReceiveE2EVerifiedRecipient,
            LockTooltip::ReceiveE2EVerificationFailed => Self::ReceiveE2EVerificationFailed,
            LockTooltip::ReceiveE2EVerificationFailedNoSignature => {
                Self::ReceiveE2EVerificationFailedNoSignature
            }
            LockTooltip::ReceiveSignOnlyVerifiedRecipient => Self::ReceiveSignOnlyVerifiedRecipient,
            LockTooltip::ReceiveSignOnlyVerificationFailed => {
                Self::ReceiveSignOnlyVerificationFailed
            }
            LockTooltip::SentE2EVerifiedRecipients => Self::SentE2EVerifiedRecipients,
            LockTooltip::SentProtonVerifiedRecipients => Self::SentProtonVerifiedRecipients,
            LockTooltip::SentE2E => Self::SentE2E,
            LockTooltip::SentRecipientE2EVerifiedRecipient => {
                Self::SentRecipientE2EVerifiedRecipient
            }
            LockTooltip::SentRecipientProtonMailVerifiedRecipient => {
                Self::SentRecipientProtonMailVerifiedRecipient
            }
            LockTooltip::SentRecipientE2E => Self::SentRecipientE2E,
            LockTooltip::SentRecipientProtonMail => Self::SentRecipientProtonMail,
            LockTooltip::SentRecipientE2EPgpVerifiedRecipient => {
                Self::SentRecipientE2EPgpVerifiedRecipient
            }
            LockTooltip::SentRecipientProtonMailPgpVerifiedRecipient => {
                Self::SentRecipientProtonMailPgpVerifiedRecipient
            }
            LockTooltip::SentRecipientE2EPgpRecipient => Self::SentRecipientE2EPgpRecipient,
            LockTooltip::SentRecipientProtonMailPgpRecipient => {
                Self::SentRecipientProtonMailPgpRecipient
            }
            LockTooltip::SentRecipientPGPSigned => Self::SentRecipientPGPSigned,
        }
    }
}
