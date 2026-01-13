use std::{fmt::Display, str::FromStr};

use proton_crypto_account::proton_crypto::crypto::{
    PublicKey, VerificationError, VerificationResult,
};

use crate::keys::{InboxVerificationPreferences, PackageCryptoType, SendPreferences};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LockColor {
    Black,
    Green,
    Blue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LockIcon {
    ClosedLock,
    ClosedLockWithTick,
    ClosedLockWithPen,
    ClosedLockWarning,
    OpenLockWithPen,
    OpenLockWithTick,
    OpenLockWarning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LockTooltip {
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

impl Display for LockTooltip {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockTooltip::None=> Ok(()),
            LockTooltip::SendE2E | LockTooltip::SentRecipientE2E => f.write_str("End-to-end encrypted"),
            LockTooltip::SendE2EVerifiedRecipient | LockTooltip::SentRecipientE2EVerifiedRecipient => f.write_str("End-to-end encrypted to verified recipient"),
            LockTooltip::SendZeroAccessEncryptionDisabled => f.write_str("Zero-access encrypted. Recipient has disabled end-to-end encryption on their account."),
            LockTooltip::SendSignOnly | LockTooltip::SentRecipientPGPSigned => f.write_str("PGP-signed"),
            LockTooltip::ReceiveE2E => f.write_str("End-to-end encrypted message"),
            LockTooltip::ReceiveE2EVerifiedRecipient => f.write_str("End-to-end encrypted message from verified sender"),
            LockTooltip::ReceiveE2EVerificationFailed => f.write_str("Sender verification failed"),
            LockTooltip::ReceiveE2EVerificationFailedNoSignature => f.write_str("Sender could not be verified: Message not signed"),
            LockTooltip::ReceiveSignOnlyVerificationFailed => f.write_str("PGP-signed message. Sender verification failed"),
            LockTooltip::ZeroAccessSentByProton => f.write_str("Sent by ProtonMail with zero-access encryption"),
            LockTooltip::ReceiveSignOnlyVerifiedRecipient => f.write_str("PGP-signed message from verified sender"),
            LockTooltip::ZeroAccess => f.write_str("Stored with zero-access encryption"),
            LockTooltip::SentE2EVerifiedRecipients => f.write_str("Sent by you with end-to-end encryption to verified recipients"),
            LockTooltip::SentProtonVerifiedRecipients => f.write_str("Sent by ProtonMail with zero-access encryption to verified recipients"),
            LockTooltip::SentE2E => f.write_str("Sent by you with end-to-end encryption"),
            LockTooltip::SentRecipientProtonMailVerifiedRecipient => f.write_str("Encrypted by ProtonMail to verified recipient"),
            LockTooltip::SentRecipientProtonMail => f.write_str("Encrypted by ProtonMail"),
            LockTooltip::SentRecipientE2EPgpVerifiedRecipient => f.write_str("End-to-end encrypted to verified PGP recipient"),
            LockTooltip::SentRecipientProtonMailPgpVerifiedRecipient => f.write_str("Encrypted by ProtonMail to verified PGP recipient"),
            LockTooltip::SentRecipientE2EPgpRecipient => f.write_str("End-to-end encrypted to PGP recipient"),
            LockTooltip::SentRecipientProtonMailPgpRecipient => f.write_str("Encrypted by ProtonMail to PGP recipient"),
        }
    }
}

/// The lock icon to display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UiLock {
    pub icon: LockIcon,
    pub color: LockColor,
    pub tooltip: LockTooltip,
}

impl UiLock {
    #[must_use]
    pub fn default_incoming() -> Self {
        Self {
            icon: LockIcon::ClosedLock,
            color: LockColor::Black,
            tooltip: LockTooltip::ZeroAccess,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MailVerificationStatus {
    NotVerified,
    NotSigned,
    SignedAndValid,
    SignedAndInvalid,
    SignedNoPublicKey,
}

impl From<VerificationResult> for MailVerificationStatus {
    fn from(result: VerificationResult) -> Self {
        match result {
            Ok(_) => Self::SignedAndValid,
            Err(err) => match err {
                VerificationError::NotSigned(_) => Self::NotSigned,
                VerificationError::NoVerifier(_) => Self::SignedNoPublicKey,
                VerificationError::Failed(_, _) | VerificationError::BadContext(_, _) => {
                    Self::SignedAndInvalid
                }
                VerificationError::RuntimeError(_) => Self::NotVerified,
            },
        }
    }
}

/// Extracted from the `X-Pm-Origin` message header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum XPmOrigin {
    None,
    Internal,
    External,
    Import,
}

impl FromStr for XPmOrigin {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().trim() {
            "internal" => Ok(XPmOrigin::Internal),
            "external" => Ok(XPmOrigin::External),
            "import" => Ok(XPmOrigin::Import),
            "none" => Ok(XPmOrigin::None),
            _ => Err("Invalid origin"),
        }
    }
}

impl XPmOrigin {
    #[must_use]
    pub fn header_key() -> &'static str {
        "X-Pm-Origin"
    }
}

/// Extracted from the `X-Pm-Content-Encryption` message header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum XPmContentEncryption {
    None,
    EndToEnd,
    OnDelivery,
    OnCompose,
}

impl XPmContentEncryption {
    #[must_use]
    pub fn header_key() -> &'static str {
        "X-Pm-Content-Encryption"
    }
}

impl FromStr for XPmContentEncryption {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().trim() {
            "end-to-end" => Ok(XPmContentEncryption::EndToEnd),
            "on-delivery" => Ok(XPmContentEncryption::OnDelivery),
            "on-compose" => Ok(XPmContentEncryption::OnCompose),
            "none" => Ok(XPmContentEncryption::None),
            _ => Err("Invalid content encryption"),
        }
    }
}

impl<T: AsRef<str>> From<T> for XPmContentEncryption {
    fn from(s: T) -> Self {
        s.as_ref().parse().unwrap_or(XPmContentEncryption::None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum XPmRecipientAuthentication {
    None,
    PgpInline,
    PgpMime,
    PgpEo,
    PgpPm,
}

impl XPmRecipientAuthentication {
    /// Attempt to parse an X-Pm-Recipient-Authentication header string.
    ///
    /// E.g.:
    /// ```ignore
    /// foo%40proton.me=pgp-pm;
    ///  bar%40protonmail.com=pgp-pm
    /// ```
    pub fn from_header(value: &str) -> Result<Vec<Self>, &'static str> {
        parse_and_map_header::<Self>(value)
    }

    #[must_use]
    pub fn header_key() -> &'static str {
        "X-Pm-Recipient-Authentication"
    }
}

impl std::str::FromStr for XPmRecipientAuthentication {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().trim() {
            "pgp-inline" => Ok(XPmRecipientAuthentication::PgpInline),
            "pgp-mime" => Ok(XPmRecipientAuthentication::PgpMime),
            "pgp-eo" => Ok(XPmRecipientAuthentication::PgpEo),
            "pgp-pm" => Ok(XPmRecipientAuthentication::PgpPm),
            "none" => Ok(XPmRecipientAuthentication::None),
            _ => Err("Invalid recipient authentication"),
        }
    }
}

impl<T: AsRef<str>> From<T> for XPmRecipientAuthentication {
    fn from(s: T) -> Self {
        s.as_ref()
            .parse()
            .unwrap_or(XPmRecipientAuthentication::None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum XPmRecipientEncryption {
    None,
    PgpInline,
    PgpInlinePinned,
    PgpMime,
    PgpMimePinned,
    PgpEo,
    PgpPm,
    PgpPmPinned,
}

impl XPmRecipientEncryption {
    /// Attempt to parse an X-Pm-Recipient-Encryption header string.
    ///
    /// E.g.:
    /// ```ignore
    /// foo%40proton.me=pgp-pm;
    ///  bar%40protonmail.com=pgp-pm
    /// ```
    pub fn from_header(value: &str) -> Result<Vec<Self>, &'static str> {
        parse_and_map_header::<Self>(value)
    }

    #[must_use]
    pub fn header_key() -> &'static str {
        "X-Pm-Recipient-Encryption"
    }
}

impl std::str::FromStr for XPmRecipientEncryption {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().trim() {
            "pgp-inline" => Ok(XPmRecipientEncryption::PgpInline),
            "pgp-inline-pinned" => Ok(XPmRecipientEncryption::PgpInlinePinned),
            "pgp-mime" => Ok(XPmRecipientEncryption::PgpMime),
            "pgp-mime-pinned" => Ok(XPmRecipientEncryption::PgpMimePinned),
            "pgp-eo" => Ok(XPmRecipientEncryption::PgpEo),
            "pgp-pm" => Ok(XPmRecipientEncryption::PgpPm),
            "pgp-pm-pinned" => Ok(XPmRecipientEncryption::PgpPmPinned),
            "none" => Ok(XPmRecipientEncryption::None),
            _ => Err("Invalid recipient encryption"),
        }
    }
}

impl XPmRecipientEncryption {
    #[must_use]
    pub fn is_external(&self) -> bool {
        matches!(
            self,
            XPmRecipientEncryption::PgpInline
                | XPmRecipientEncryption::PgpInlinePinned
                | XPmRecipientEncryption::PgpMime
                | XPmRecipientEncryption::PgpMimePinned
        )
    }

    #[must_use]
    pub fn is_internal(&self) -> bool {
        matches!(
            self,
            XPmRecipientEncryption::PgpPmPinned
                | XPmRecipientEncryption::PgpPm
                | XPmRecipientEncryption::PgpEo
        )
    }

    #[must_use]
    pub fn is_pinned(&self) -> bool {
        matches!(
            self,
            XPmRecipientEncryption::PgpInlinePinned
                | XPmRecipientEncryption::PgpMimePinned
                | XPmRecipientEncryption::PgpPmPinned
        )
    }
}

impl UiLock {
    /// Determines the lock icon for a message that is sent from the composer.
    #[must_use]
    pub fn for_composer<Pub>(send_prefs: &SendPreferences<Pub>) -> Option<Self>
    where
        Pub: PublicKey,
    {
        determine_composer_lock_icon(send_prefs)
    }

    /// Determines the lock icon for a message received in the inbox that is not in the sent folder.
    ///
    /// The [`MailVerificationStatus`] can be created from a [`VerificationResult`], which is the
    /// output of the signature verification.
    #[must_use]
    pub fn for_receive_inbox<Pub>(
        origin_header: XPmOrigin,
        content_encryption_header: XPmContentEncryption,
        message_verification_status: MailVerificationStatus,
        verification_preferences_opt: Option<&InboxVerificationPreferences<Pub>>,
    ) -> Self
    where
        Pub: PublicKey,
    {
        determine_recipient_lock_icon(
            origin_header,
            content_encryption_header,
            message_verification_status,
            verification_preferences_opt,
        )
    }

    /// Determines the aggregated lock icon for a message that was sent by the user.
    ///
    /// `per_recipient_encryption` contains the message header for each recipient of the email.
    #[must_use]
    pub fn for_sent_inbox(
        origin_header: XPmOrigin,
        content_encryption: XPmContentEncryption,
        per_recipient_encryption: &[XPmRecipientEncryption],
    ) -> UiLock {
        determine_sent_lock_icon_aggregated(
            origin_header,
            content_encryption,
            per_recipient_encryption,
        )
    }

    /// Determines a per-recipient lock icon for a message that was sent by the user.
    #[must_use]
    pub fn for_sent_inbox_per_recipient(
        content_encryption: XPmContentEncryption,
        recipient_encryption: XPmRecipientEncryption,
        recipient_authentication: XPmRecipientAuthentication,
    ) -> UiLock {
        determine_sent_lock_icon_for_recipient(
            content_encryption,
            recipient_encryption,
            recipient_authentication,
        )
    }
}

fn determine_composer_lock_icon<Pub>(send_prefs: &SendPreferences<Pub>) -> Option<UiLock>
where
    Pub: PublicKey,
{
    if send_prefs.encryption_disabled {
        return Some(UiLock {
            icon: LockIcon::ClosedLock,
            color: LockColor::Black,
            tooltip: LockTooltip::SendZeroAccessEncryptionDisabled,
        });
    }
    let (icon, color) = match send_prefs.pgp_scheme {
        PackageCryptoType::ProtonMail => {
            let color = LockColor::Blue;
            let lock_icon = if send_prefs.encrypt && send_prefs.is_selected_key_pinned {
                LockIcon::ClosedLockWithTick
            } else if send_prefs.encrypt {
                LockIcon::ClosedLock
            } else {
                return None;
            };
            (lock_icon, color)
        }
        PackageCryptoType::EncryptedOutside => (LockIcon::ClosedLock, LockColor::Blue),
        PackageCryptoType::Cleartext => return None,
        PackageCryptoType::PgpInline
        | PackageCryptoType::PgpMime
        | PackageCryptoType::ClearMime => {
            let color = LockColor::Green;
            #[allow(clippy::match_same_arms)]
            let icon = match (
                send_prefs.encrypt,
                send_prefs.sign,
                send_prefs.is_selected_key_pinned,
            ) {
                (true, true, true) => LockIcon::ClosedLockWithTick,
                (true, true, false) => LockIcon::ClosedLock,
                (false, true, true) => LockIcon::OpenLockWithTick,
                (false, true, false) => LockIcon::OpenLockWithPen,
                (true, _, _) => LockIcon::ClosedLock,
                (false, _, _) => return None,
            };
            (icon, color)
        }
    };
    Some(UiLock {
        icon,
        color,
        tooltip: tooltip_composer(icon),
    })
}

#[must_use]
#[allow(clippy::too_many_lines)]
fn determine_recipient_lock_icon<Pub>(
    origin_header: XPmOrigin,
    content_encryption_header: XPmContentEncryption,
    message_verification_status: MailVerificationStatus,
    verification_preferences_opt: Option<&InboxVerificationPreferences<Pub>>,
) -> UiLock
where
    Pub: PublicKey,
{
    use MailVerificationStatus::{
        NotSigned, NotVerified, SignedAndInvalid, SignedAndValid, SignedNoPublicKey,
    };
    let pinned =
        verification_preferences_opt.is_some_and(InboxVerificationPreferences::uses_pinned_keys);

    let self_owned_keys =
        verification_preferences_opt.is_some_and(InboxVerificationPreferences::self_owned_keys);

    match (origin_header, content_encryption_header) {
        (XPmOrigin::Internal, XPmContentEncryption::EndToEnd) => {
            let color = LockColor::Blue;
            if pinned || self_owned_keys {
                match message_verification_status {
                    NotVerified | SignedNoPublicKey => UiLock {
                        icon: LockIcon::ClosedLock,
                        color,
                        tooltip: LockTooltip::ReceiveE2E,
                    },
                    NotSigned => UiLock {
                        icon: LockIcon::ClosedLockWarning,
                        color,
                        tooltip: LockTooltip::ReceiveE2EVerificationFailedNoSignature,
                    },
                    SignedAndValid => UiLock {
                        icon: LockIcon::ClosedLockWithTick,
                        color,
                        tooltip: LockTooltip::ReceiveE2EVerifiedRecipient,
                    },
                    SignedAndInvalid => UiLock {
                        icon: LockIcon::ClosedLockWarning,
                        color,
                        tooltip: LockTooltip::ReceiveE2EVerificationFailed,
                    },
                }
            } else {
                UiLock {
                    icon: LockIcon::ClosedLock,
                    color,
                    tooltip: LockTooltip::ReceiveE2E,
                }
            }
        }
        (XPmOrigin::Internal, XPmContentEncryption::OnDelivery) => UiLock {
            icon: LockIcon::ClosedLock,
            color: LockColor::Black,
            tooltip: LockTooltip::ZeroAccessSentByProton,
        },
        (XPmOrigin::External, XPmContentEncryption::EndToEnd) => {
            let color = LockColor::Green;
            if pinned {
                match message_verification_status {
                    NotVerified | SignedNoPublicKey | NotSigned => UiLock {
                        icon: LockIcon::ClosedLock,
                        color,
                        tooltip: LockTooltip::ReceiveE2E,
                    },
                    SignedAndValid => UiLock {
                        icon: LockIcon::ClosedLockWithTick,
                        color,
                        tooltip: LockTooltip::ReceiveE2EVerifiedRecipient,
                    },
                    SignedAndInvalid => UiLock {
                        icon: LockIcon::ClosedLockWarning,
                        color,
                        tooltip: LockTooltip::ReceiveE2EVerificationFailed,
                    },
                }
            } else {
                UiLock {
                    icon: LockIcon::ClosedLock,
                    color,
                    tooltip: LockTooltip::ReceiveE2E,
                }
            }
        }
        (XPmOrigin::External, XPmContentEncryption::OnDelivery) => {
            if pinned {
                match message_verification_status {
                    NotSigned | NotVerified | SignedNoPublicKey => UiLock {
                        icon: LockIcon::ClosedLock,
                        color: LockColor::Black,
                        tooltip: LockTooltip::ZeroAccess,
                    },
                    SignedAndValid => UiLock {
                        icon: LockIcon::OpenLockWithTick,
                        color: LockColor::Green,
                        tooltip: LockTooltip::ReceiveSignOnlyVerifiedRecipient,
                    },
                    SignedAndInvalid => UiLock {
                        icon: LockIcon::OpenLockWarning,
                        color: LockColor::Green,
                        tooltip: LockTooltip::ReceiveSignOnlyVerificationFailed,
                    },
                }
            } else {
                UiLock {
                    icon: LockIcon::ClosedLock,
                    color: LockColor::Black,
                    tooltip: LockTooltip::ZeroAccess,
                }
            }
        }
        _ => UiLock {
            icon: LockIcon::ClosedLock,
            color: LockColor::Black,
            tooltip: LockTooltip::ZeroAccess,
        },
    }
}

#[must_use]
fn determine_sent_lock_icon_aggregated(
    origin_header: XPmOrigin,
    content_encryption: XPmContentEncryption,
    per_recipient_encryption: &[XPmRecipientEncryption],
) -> UiLock {
    let all_external = !per_recipient_encryption.is_empty()
        && per_recipient_encryption
            .iter()
            .all(XPmRecipientEncryption::is_external);
    let all_pinned = !per_recipient_encryption.is_empty()
        && per_recipient_encryption
            .iter()
            .all(XPmRecipientEncryption::is_pinned);
    let all_encrypted = !per_recipient_encryption.is_empty()
        && !per_recipient_encryption.contains(&XPmRecipientEncryption::None);

    let color = if all_external {
        LockColor::Green
    } else {
        LockColor::Blue
    };
    if all_pinned {
        let tooltip = if content_encryption == XPmContentEncryption::EndToEnd {
            LockTooltip::SentE2EVerifiedRecipients
        } else {
            LockTooltip::SentProtonVerifiedRecipients
        };
        return UiLock {
            icon: LockIcon::ClosedLockWithTick,
            color,
            tooltip,
        };
    }
    if all_encrypted {
        let tooltip = if content_encryption == XPmContentEncryption::EndToEnd {
            LockTooltip::SentE2E
        } else {
            LockTooltip::ZeroAccessSentByProton
        };
        return UiLock {
            icon: LockIcon::ClosedLock,
            color,
            tooltip,
        };
    }
    let color = if origin_header == XPmOrigin::Import {
        LockColor::Black
    } else {
        LockColor::Blue
    };
    UiLock {
        icon: LockIcon::ClosedLock,
        color,
        tooltip: LockTooltip::ZeroAccess,
    }
}

#[must_use]
fn determine_sent_lock_icon_for_recipient(
    content_encryption: XPmContentEncryption,
    recipient_encryption: XPmRecipientEncryption,
    recipient_authentication: XPmRecipientAuthentication,
) -> UiLock {
    #[allow(clippy::match_same_arms)]
    let (icon, color) = match (recipient_encryption, recipient_authentication) {
        (
            XPmRecipientEncryption::None,
            XPmRecipientAuthentication::PgpInline | XPmRecipientAuthentication::PgpMime,
        ) => (LockIcon::OpenLockWithPen, LockColor::Green),
        (
            XPmRecipientEncryption::PgpInline,
            XPmRecipientAuthentication::None | XPmRecipientAuthentication::PgpInline,
        ) => (LockIcon::ClosedLock, LockColor::Green),
        (
            XPmRecipientEncryption::PgpInlinePinned,
            XPmRecipientAuthentication::None | XPmRecipientAuthentication::PgpInline,
        ) => (LockIcon::ClosedLockWithTick, LockColor::Green),
        (
            XPmRecipientEncryption::PgpMime,
            XPmRecipientAuthentication::None | XPmRecipientAuthentication::PgpMime,
        ) => (LockIcon::ClosedLock, LockColor::Green),
        (
            XPmRecipientEncryption::PgpMimePinned,
            XPmRecipientAuthentication::None | XPmRecipientAuthentication::PgpMime,
        ) => (LockIcon::ClosedLockWithTick, LockColor::Green),
        (XPmRecipientEncryption::PgpEo, XPmRecipientAuthentication::PgpEo) => {
            (LockIcon::ClosedLock, LockColor::Blue)
        }
        (XPmRecipientEncryption::PgpPm, XPmRecipientAuthentication::PgpPm) => {
            (LockIcon::ClosedLock, LockColor::Blue)
        }
        (XPmRecipientEncryption::PgpPmPinned, XPmRecipientAuthentication::PgpPm) => {
            (LockIcon::ClosedLockWithTick, LockColor::Blue)
        }
        _ => (LockIcon::ClosedLock, LockColor::Black),
    };

    let tooltip = match (icon, color, content_encryption) {
        (LockIcon::ClosedLockWithTick, LockColor::Blue, XPmContentEncryption::EndToEnd) => {
            LockTooltip::SentRecipientE2EVerifiedRecipient
        }
        (
            LockIcon::ClosedLockWithTick,
            LockColor::Blue,
            XPmContentEncryption::OnDelivery | XPmContentEncryption::OnCompose,
        ) => LockTooltip::SentRecipientProtonMailVerifiedRecipient,
        (LockIcon::ClosedLock, LockColor::Blue, XPmContentEncryption::EndToEnd) => {
            LockTooltip::SentRecipientE2E
        }
        (
            LockIcon::ClosedLock,
            LockColor::Blue,
            XPmContentEncryption::OnDelivery | XPmContentEncryption::OnCompose,
        ) => LockTooltip::SentRecipientProtonMail,
        (LockIcon::ClosedLockWithTick, LockColor::Green, XPmContentEncryption::EndToEnd) => {
            LockTooltip::SentRecipientE2EPgpVerifiedRecipient
        }
        (
            LockIcon::ClosedLockWithTick,
            LockColor::Green,
            XPmContentEncryption::OnDelivery | XPmContentEncryption::OnCompose,
        ) => LockTooltip::SentRecipientProtonMailPgpVerifiedRecipient,
        (LockIcon::ClosedLock, LockColor::Green, XPmContentEncryption::EndToEnd) => {
            LockTooltip::SentRecipientE2EPgpRecipient
        }
        (
            LockIcon::ClosedLock,
            LockColor::Green,
            XPmContentEncryption::OnDelivery | XPmContentEncryption::OnCompose,
        ) => LockTooltip::SentRecipientProtonMailPgpRecipient,
        (LockIcon::OpenLockWithPen, LockColor::Green, XPmContentEncryption::EndToEnd) => {
            LockTooltip::SentRecipientPGPSigned
        }
        _ => LockTooltip::ZeroAccess,
    };
    UiLock {
        icon,
        color,
        tooltip,
    }
}

fn tooltip_composer(lock: LockIcon) -> LockTooltip {
    match lock {
        LockIcon::ClosedLock => LockTooltip::SendE2E,
        LockIcon::ClosedLockWithTick => LockTooltip::SendE2EVerifiedRecipient,
        LockIcon::OpenLockWithPen => LockTooltip::SendSignOnly,
        _ => LockTooltip::None,
    }
}

fn parse_and_map_header<T>(value: &str) -> Result<Vec<T>, <T as FromStr>::Err>
where
    T: FromStr<Err = &'static str> + 'static,
{
    value
        .split(';')
        .filter_map(|item| {
            let trimmed = item.trim();
            (!trimmed.is_empty()).then_some(trimmed)
        })
        .map(|item| {
            let (_, value) = item.rsplit_once('=').ok_or("Invalid format")?;
            T::from_str(value)
        })
        .collect::<Result<Vec<_>, &'static str>>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_x_pm_recipient_encryption_single() {
        let value = "foo%40proton.me=pgp-pm";
        let parsed = XPmRecipientEncryption::from_header(value).unwrap();
        assert_eq!(parsed, vec![XPmRecipientEncryption::PgpPm]);
    }

    #[test]
    fn parse_x_pm_recipient_encryption_list() {
        let value = "foo%40proton.me=pgp-pm;\n bar%40protonmail.com=pgp-mime";
        let parsed = XPmRecipientEncryption::from_header(value).unwrap();
        assert_eq!(
            parsed,
            vec![
                XPmRecipientEncryption::PgpPm,
                XPmRecipientEncryption::PgpMime
            ]
        );
    }

    #[test]
    fn parse_x_pm_recipient_authentication_single() {
        let value = "foo%40proton.me=pgp-pm";
        let parsed = XPmRecipientAuthentication::from_header(value).unwrap();
        assert_eq!(parsed, vec![XPmRecipientAuthentication::PgpPm]);
    }

    #[test]
    fn parse_x_pm_recipient_authentication_list() {
        let value = "foo%40proton.me=pgp-pm;\n bar%40protonmail.com=pgp-mime";
        let parsed = XPmRecipientAuthentication::from_header(value).unwrap();
        assert_eq!(
            parsed,
            vec![
                XPmRecipientAuthentication::PgpPm,
                XPmRecipientAuthentication::PgpMime
            ]
        );
    }
}
