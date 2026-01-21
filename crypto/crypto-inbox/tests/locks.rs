use std::collections::HashSet;

use proton_crypto_inbox::{
    keys::{InboxVerificationPreferences, KeyOwnership, PackageCryptoType, SendPreferences},
    lock_icon::{
        LockColor, LockIcon, LockTooltip, MailVerificationStatus, UiLock, XPmContentEncryption,
        XPmOrigin, XPmRecipientAuthentication, XPmRecipientEncryption,
    },
    message::packages::PackageMimeType,
    proton_crypto::{
        crypto::{DataEncoding, PGPProviderSync, PublicKey},
        keytransparency::VerificationError,
        new_pgp_provider,
    },
};

mod common;

#[test]
fn composer_lock_icon_internal() {
    let pgp = new_pgp_provider();
    let send_prefs = create_send_prefs(
        &pgp,
        true,
        true,
        PackageCryptoType::ProtonMail,
        PackageMimeType::Html,
        false,
        false,
    );
    perform_composer_lock_icon_test(
        &send_prefs,
        Some(UiLock {
            icon: LockIcon::ClosedLock,
            color: LockColor::Blue,
            tooltip: LockTooltip::SendE2E,
        }),
    );
}

#[test]
fn composer_lock_icon_internal_pinned() {
    let pgp = new_pgp_provider();
    let send_prefs = create_send_prefs(
        &pgp,
        true,
        true,
        PackageCryptoType::ProtonMail,
        PackageMimeType::Html,
        true,
        false,
    );
    perform_composer_lock_icon_test(
        &send_prefs,
        Some(UiLock {
            icon: LockIcon::ClosedLockWithTick,
            color: LockColor::Blue,
            tooltip: LockTooltip::SendE2EVerifiedRecipient,
        }),
    );
}

#[test]
fn composer_lock_icon_internal_encryption_disabled() {
    let pgp = new_pgp_provider();
    let send_prefs = create_send_prefs(
        &pgp,
        false,
        false,
        PackageCryptoType::ProtonMail,
        PackageMimeType::Html,
        false,
        true,
    );
    perform_composer_lock_icon_test(
        &send_prefs,
        Some(UiLock {
            icon: LockIcon::ClosedLock,
            color: LockColor::Black,
            tooltip: LockTooltip::SendZeroAccessEncryptionDisabled,
        }),
    );
}

#[test]
fn composer_lock_external_e2e() {
    let pgp = new_pgp_provider();
    let send_prefs = create_send_prefs(
        &pgp,
        true,
        true,
        PackageCryptoType::PgpMime,
        PackageMimeType::Html,
        false,
        false,
    );
    perform_composer_lock_icon_test(
        &send_prefs,
        Some(UiLock {
            icon: LockIcon::ClosedLock,
            color: LockColor::Green,
            tooltip: LockTooltip::SendE2E,
        }),
    );
}

#[test]
fn composer_lock_external_e2e_pinned() {
    let pgp = new_pgp_provider();
    let send_prefs = create_send_prefs(
        &pgp,
        true,
        true,
        PackageCryptoType::PgpMime,
        PackageMimeType::Html,
        true,
        false,
    );
    perform_composer_lock_icon_test(
        &send_prefs,
        Some(UiLock {
            icon: LockIcon::ClosedLockWithTick,
            color: LockColor::Green,
            tooltip: LockTooltip::SendE2EVerifiedRecipient,
        }),
    );
}

#[test]
fn composer_lock_external_sign_only() {
    let pgp = new_pgp_provider();
    let send_prefs = create_send_prefs(
        &pgp,
        false,
        true,
        PackageCryptoType::ClearMime,
        PackageMimeType::Html,
        false,
        false,
    );
    perform_composer_lock_icon_test(
        &send_prefs,
        Some(UiLock {
            icon: LockIcon::OpenLockWithPen,
            color: LockColor::Green,
            tooltip: LockTooltip::SendSignOnly,
        }),
    );
}

#[test]
fn composer_lock_encrypt_to_outside() {
    let pgp = new_pgp_provider();
    let send_prefs = create_send_prefs(
        &pgp,
        true,
        false,
        PackageCryptoType::EncryptedOutside,
        PackageMimeType::Html,
        false,
        false,
    );
    perform_composer_lock_icon_test(
        &send_prefs,
        Some(UiLock {
            icon: LockIcon::ClosedLock,
            color: LockColor::Blue,
            tooltip: LockTooltip::SendE2E,
        }),
    );
}

#[test]
fn composer_lock_no_encryption() {
    let pgp = new_pgp_provider();
    let send_prefs = create_send_prefs(
        &pgp,
        false,
        false,
        PackageCryptoType::Cleartext,
        PackageMimeType::Html,
        false,
        false,
    );
    perform_composer_lock_icon_test(&send_prefs, None);
}

#[test]
fn recipient_lock_icon_internal_e2e() {
    let pgp = new_pgp_provider();
    let params = recipient_lock_icon_test_params(
        &pgp,
        XPmOrigin::Internal,
        XPmContentEncryption::EndToEnd,
        MailVerificationStatus::NotVerified,
        false,
    );
    perform_recipient_lock_icon_test(
        &params,
        UiLock {
            icon: LockIcon::ClosedLock,
            color: LockColor::Blue,
            tooltip: LockTooltip::ReceiveE2E,
        },
    );
}

#[test]
fn recipient_lock_icon_internal_e2e_pinned() {
    let pgp = new_pgp_provider();
    let params = recipient_lock_icon_test_params(
        &pgp,
        XPmOrigin::Internal,
        XPmContentEncryption::EndToEnd,
        MailVerificationStatus::SignedAndValid,
        true,
    );
    perform_recipient_lock_icon_test(
        &params,
        UiLock {
            icon: LockIcon::ClosedLockWithTick,
            color: LockColor::Blue,
            tooltip: LockTooltip::ReceiveE2EVerifiedRecipient,
        },
    );
}

#[test]
fn recipient_lock_icon_internal_e2e_pinned_failed() {
    let pgp = new_pgp_provider();
    let params = recipient_lock_icon_test_params(
        &pgp,
        XPmOrigin::Internal,
        XPmContentEncryption::EndToEnd,
        MailVerificationStatus::SignedAndInvalid,
        true,
    );
    perform_recipient_lock_icon_test(
        &params,
        UiLock {
            icon: LockIcon::ClosedLockWarning,
            color: LockColor::Blue,
            tooltip: LockTooltip::ReceiveE2EVerificationFailed,
        },
    );
}

#[test]
fn recipient_lock_icon_internal_e2e_pinned_failed_no_matching_key() {
    let pgp = new_pgp_provider();
    let params = recipient_lock_icon_test_params(
        &pgp,
        XPmOrigin::Internal,
        XPmContentEncryption::EndToEnd,
        MailVerificationStatus::SignedNoPublicKey,
        true,
    );
    perform_recipient_lock_icon_test(
        &params,
        UiLock {
            icon: LockIcon::ClosedLockWarning,
            color: LockColor::Blue,
            tooltip: LockTooltip::ReceiveE2EVerificationFailed,
        },
    );
}

#[test]
fn recipient_lock_icon_internal_e2e_pinned_not_signed() {
    let pgp = new_pgp_provider();
    let params = recipient_lock_icon_test_params(
        &pgp,
        XPmOrigin::Internal,
        XPmContentEncryption::EndToEnd,
        MailVerificationStatus::NotSigned,
        true,
    );
    perform_recipient_lock_icon_test(
        &params,
        UiLock {
            icon: LockIcon::ClosedLockWarning,
            color: LockColor::Blue,
            tooltip: LockTooltip::ReceiveE2EVerificationFailedNoSignature,
        },
    );
}

#[test]
fn recipient_lock_icon_external_e2e() {
    let pgp = new_pgp_provider();
    let params = recipient_lock_icon_test_params(
        &pgp,
        XPmOrigin::External,
        XPmContentEncryption::EndToEnd,
        MailVerificationStatus::NotVerified,
        false,
    );
    perform_recipient_lock_icon_test(
        &params,
        UiLock {
            icon: LockIcon::ClosedLock,
            color: LockColor::Green,
            tooltip: LockTooltip::ReceiveE2E,
        },
    );
}

#[test]
fn recipient_lock_icon_external_e2e_pinned() {
    let pgp = new_pgp_provider();
    let params = recipient_lock_icon_test_params(
        &pgp,
        XPmOrigin::External,
        XPmContentEncryption::EndToEnd,
        MailVerificationStatus::SignedAndValid,
        true,
    );
    perform_recipient_lock_icon_test(
        &params,
        UiLock {
            icon: LockIcon::ClosedLockWithTick,
            color: LockColor::Green,
            tooltip: LockTooltip::ReceiveE2EVerifiedRecipient,
        },
    );
}

#[test]
fn recipient_lock_icon_external_e2e_pinned_no_signature() {
    let pgp = new_pgp_provider();
    let params = recipient_lock_icon_test_params(
        &pgp,
        XPmOrigin::External,
        XPmContentEncryption::EndToEnd,
        MailVerificationStatus::NotSigned,
        true,
    );
    perform_recipient_lock_icon_test(
        &params,
        UiLock {
            icon: LockIcon::ClosedLock,
            color: LockColor::Green,
            tooltip: LockTooltip::ReceiveE2E,
        },
    );
}

#[test]
fn recipient_lock_icon_external_e2e_pinned_failed() {
    let pgp = new_pgp_provider();
    let params = recipient_lock_icon_test_params(
        &pgp,
        XPmOrigin::External,
        XPmContentEncryption::EndToEnd,
        MailVerificationStatus::SignedAndInvalid,
        true,
    );
    perform_recipient_lock_icon_test(
        &params,
        UiLock {
            icon: LockIcon::ClosedLockWarning,
            color: LockColor::Green,
            tooltip: LockTooltip::ReceiveE2EVerificationFailed,
        },
    );
}

#[test]
fn recipient_lock_icon_external_e2e_pinned_failed_no_matching_key() {
    let pgp = new_pgp_provider();
    let params = recipient_lock_icon_test_params(
        &pgp,
        XPmOrigin::External,
        XPmContentEncryption::EndToEnd,
        MailVerificationStatus::SignedNoPublicKey,
        true,
    );
    perform_recipient_lock_icon_test(
        &params,
        UiLock {
            icon: LockIcon::ClosedLockWarning,
            color: LockColor::Green,
            tooltip: LockTooltip::ReceiveE2EVerificationFailed,
        },
    );
}

#[test]
fn recipient_lock_icon_external_sign_only_pinned() {
    let pgp = new_pgp_provider();
    let params = recipient_lock_icon_test_params(
        &pgp,
        XPmOrigin::External,
        XPmContentEncryption::OnDelivery,
        MailVerificationStatus::SignedAndValid,
        true,
    );
    perform_recipient_lock_icon_test(
        &params,
        UiLock {
            icon: LockIcon::OpenLockWithTick,
            color: LockColor::Green,
            tooltip: LockTooltip::ReceiveSignOnlyVerifiedRecipient,
        },
    );
}

#[test]
fn recipient_lock_icon_external_sign_only_pinned_failure() {
    let pgp = new_pgp_provider();
    let params = recipient_lock_icon_test_params(
        &pgp,
        XPmOrigin::External,
        XPmContentEncryption::OnDelivery,
        MailVerificationStatus::SignedAndInvalid,
        true,
    );
    perform_recipient_lock_icon_test(
        &params,
        UiLock {
            icon: LockIcon::OpenLockWarning,
            color: LockColor::Green,
            tooltip: LockTooltip::ReceiveSignOnlyVerificationFailed,
        },
    );
}

#[test]
fn recipient_lock_icon_external_sign_only_pinned_failure_key_missmatch() {
    let pgp = new_pgp_provider();
    let params = recipient_lock_icon_test_params(
        &pgp,
        XPmOrigin::External,
        XPmContentEncryption::OnDelivery,
        MailVerificationStatus::SignedNoPublicKey,
        true,
    );
    perform_recipient_lock_icon_test(
        &params,
        UiLock {
            icon: LockIcon::OpenLockWarning,
            color: LockColor::Green,
            tooltip: LockTooltip::ReceiveSignOnlyVerificationFailed,
        },
    );
}

#[test]
fn recipient_lock_icon_external_no_encryption() {
    let pgp = new_pgp_provider();
    let params = recipient_lock_icon_test_params(
        &pgp,
        XPmOrigin::External,
        XPmContentEncryption::OnDelivery,
        MailVerificationStatus::NotVerified,
        false,
    );
    perform_recipient_lock_icon_test(
        &params,
        UiLock {
            icon: LockIcon::ClosedLock,
            color: LockColor::Black,
            tooltip: LockTooltip::ZeroAccess,
        },
    );
}

#[test]
fn sent_lock_icon_aggregated_all_external() {
    let per_recipient_encryption = vec![
        XPmRecipientEncryption::PgpInline,
        XPmRecipientEncryption::PgpMime,
    ];
    perform_sent_lock_icon_test(
        XPmOrigin::External,
        XPmContentEncryption::EndToEnd,
        &per_recipient_encryption,
        UiLock {
            icon: LockIcon::ClosedLock,
            color: LockColor::Green,
            tooltip: LockTooltip::SentE2E,
        },
    );
}

#[test]
fn sent_lock_icon_aggregated_all_external_pinned() {
    let per_recipient_encryption = vec![
        XPmRecipientEncryption::PgpInlinePinned,
        XPmRecipientEncryption::PgpMimePinned,
    ];
    perform_sent_lock_icon_test(
        XPmOrigin::External,
        XPmContentEncryption::EndToEnd,
        &per_recipient_encryption,
        UiLock {
            icon: LockIcon::ClosedLockWithTick,
            color: LockColor::Green,
            tooltip: LockTooltip::SentE2EVerifiedRecipients,
        },
    );
}

#[test]
fn sent_lock_icon_aggregated_all_external_pinned_proton() {
    let per_recipient_encryption = vec![
        XPmRecipientEncryption::PgpInlinePinned,
        XPmRecipientEncryption::PgpMimePinned,
    ];
    perform_sent_lock_icon_test(
        XPmOrigin::External,
        XPmContentEncryption::OnDelivery,
        &per_recipient_encryption,
        UiLock {
            icon: LockIcon::ClosedLockWithTick,
            color: LockColor::Green,
            tooltip: LockTooltip::SentProtonVerifiedRecipients,
        },
    );
}

#[test]
fn sent_lock_icon_aggregated_all_external_proton() {
    let per_recipient_encryption = vec![
        XPmRecipientEncryption::PgpInline,
        XPmRecipientEncryption::PgpMimePinned,
    ];
    perform_sent_lock_icon_test(
        XPmOrigin::External,
        XPmContentEncryption::OnDelivery,
        &per_recipient_encryption,
        UiLock {
            icon: LockIcon::ClosedLock,
            color: LockColor::Green,
            tooltip: LockTooltip::ZeroAccessSentByProton,
        },
    );
}

#[test]
fn sent_lock_icon_aggregated_internal() {
    let per_recipient_encryption = vec![
        XPmRecipientEncryption::PgpPm,
        XPmRecipientEncryption::PgpMimePinned,
    ];
    perform_sent_lock_icon_test(
        XPmOrigin::Internal,
        XPmContentEncryption::EndToEnd,
        &per_recipient_encryption,
        UiLock {
            icon: LockIcon::ClosedLock,
            color: LockColor::Blue,
            tooltip: LockTooltip::SentE2E,
        },
    );
}

#[test]
fn sent_lock_icon_aggregated_internal_porotn() {
    let per_recipient_encryption = vec![
        XPmRecipientEncryption::PgpPm,
        XPmRecipientEncryption::PgpMimePinned,
    ];
    perform_sent_lock_icon_test(
        XPmOrigin::Internal,
        XPmContentEncryption::OnDelivery,
        &per_recipient_encryption,
        UiLock {
            icon: LockIcon::ClosedLock,
            color: LockColor::Blue,
            tooltip: LockTooltip::ZeroAccessSentByProton,
        },
    );
}

#[test]
fn sent_lock_icon_aggregated_internal_pinned() {
    let per_recipient_encryption = vec![
        XPmRecipientEncryption::PgpPmPinned,
        XPmRecipientEncryption::PgpMimePinned,
    ];
    perform_sent_lock_icon_test(
        XPmOrigin::Internal,
        XPmContentEncryption::EndToEnd,
        &per_recipient_encryption,
        UiLock {
            icon: LockIcon::ClosedLockWithTick,
            color: LockColor::Blue,
            tooltip: LockTooltip::SentE2EVerifiedRecipients,
        },
    );
}

#[test]
fn sent_lock_icon_aggregated_internal_pinned_proton() {
    let per_recipient_encryption = vec![
        XPmRecipientEncryption::PgpPmPinned,
        XPmRecipientEncryption::PgpMimePinned,
    ];
    perform_sent_lock_icon_test(
        XPmOrigin::Internal,
        XPmContentEncryption::OnDelivery,
        &per_recipient_encryption,
        UiLock {
            icon: LockIcon::ClosedLockWithTick,
            color: LockColor::Blue,
            tooltip: LockTooltip::SentProtonVerifiedRecipients,
        },
    );
}

#[test]
fn sent_lock_icon_aggregated_no_encrpytion() {
    let per_recipient_encryption = vec![
        XPmRecipientEncryption::None,
        XPmRecipientEncryption::PgpMimePinned,
    ];
    perform_sent_lock_icon_test(
        XPmOrigin::External,
        XPmContentEncryption::EndToEnd,
        &per_recipient_encryption,
        UiLock {
            icon: LockIcon::ClosedLock,
            color: LockColor::Blue,
            tooltip: LockTooltip::ZeroAccess,
        },
    );
}

#[test]
fn sent_lock_icon_aggregated_no_encrpytion_imported() {
    let per_recipient_encryption = vec![
        XPmRecipientEncryption::None,
        XPmRecipientEncryption::PgpMimePinned,
    ];
    perform_sent_lock_icon_test(
        XPmOrigin::Import,
        XPmContentEncryption::EndToEnd,
        &per_recipient_encryption,
        UiLock {
            icon: LockIcon::ClosedLock,
            color: LockColor::Black,
            tooltip: LockTooltip::ZeroAccess,
        },
    );
}

#[test]
fn sent_lock_icon_recipient_blue_checkmark_e2e() {
    perform_sent_recipient_lock_icon_test(
        XPmContentEncryption::EndToEnd,
        XPmRecipientEncryption::PgpPmPinned,
        XPmRecipientAuthentication::PgpPm,
        UiLock {
            icon: LockIcon::ClosedLockWithTick,
            color: LockColor::Blue,
            tooltip: LockTooltip::SentRecipientE2EVerifiedRecipient,
        },
    );
}

#[test]
fn sent_lock_icon_recipient_blue_checkmark_ondelivery() {
    perform_sent_recipient_lock_icon_test(
        XPmContentEncryption::OnDelivery,
        XPmRecipientEncryption::PgpPmPinned,
        XPmRecipientAuthentication::PgpPm,
        UiLock {
            icon: LockIcon::ClosedLockWithTick,
            color: LockColor::Blue,
            tooltip: LockTooltip::SentRecipientProtonMailVerifiedRecipient,
        },
    );
}

#[test]
fn sent_lock_icon_recipient_blue_plain_e2e() {
    perform_sent_recipient_lock_icon_test(
        XPmContentEncryption::EndToEnd,
        XPmRecipientEncryption::PgpPm,
        XPmRecipientAuthentication::PgpPm,
        UiLock {
            icon: LockIcon::ClosedLock,
            color: LockColor::Blue,
            tooltip: LockTooltip::SentRecipientE2E,
        },
    );
}

#[test]
fn sent_lock_icon_recipient_blue_plain_ondelivery() {
    perform_sent_recipient_lock_icon_test(
        XPmContentEncryption::OnDelivery,
        XPmRecipientEncryption::PgpPm,
        XPmRecipientAuthentication::PgpPm,
        UiLock {
            icon: LockIcon::ClosedLock,
            color: LockColor::Blue,
            tooltip: LockTooltip::SentRecipientProtonMail,
        },
    );
}

#[test]
fn sent_lock_icon_recipient_green_checkmark_e2e() {
    perform_sent_recipient_lock_icon_test(
        XPmContentEncryption::EndToEnd,
        XPmRecipientEncryption::PgpMimePinned,
        XPmRecipientAuthentication::PgpMime,
        UiLock {
            icon: LockIcon::ClosedLockWithTick,
            color: LockColor::Green,
            tooltip: LockTooltip::SentRecipientE2EPgpVerifiedRecipient,
        },
    );
}

#[test]
fn sent_lock_icon_recipient_green_checkmark_ondelivery() {
    perform_sent_recipient_lock_icon_test(
        XPmContentEncryption::OnDelivery,
        XPmRecipientEncryption::PgpMimePinned,
        XPmRecipientAuthentication::PgpMime,
        UiLock {
            icon: LockIcon::ClosedLockWithTick,
            color: LockColor::Green,
            tooltip: LockTooltip::SentRecipientProtonMailPgpVerifiedRecipient,
        },
    );
}

#[test]
fn sent_lock_icon_recipient_green_plain_e2e() {
    perform_sent_recipient_lock_icon_test(
        XPmContentEncryption::EndToEnd,
        XPmRecipientEncryption::PgpMime,
        XPmRecipientAuthentication::PgpMime,
        UiLock {
            icon: LockIcon::ClosedLock,
            color: LockColor::Green,
            tooltip: LockTooltip::SentRecipientE2EPgpRecipient,
        },
    );
}

#[test]
fn sent_lock_icon_recipient_green_plain_ondelivery() {
    perform_sent_recipient_lock_icon_test(
        XPmContentEncryption::OnDelivery,
        XPmRecipientEncryption::PgpMime,
        XPmRecipientAuthentication::PgpMime,
        UiLock {
            icon: LockIcon::ClosedLock,
            color: LockColor::Green,
            tooltip: LockTooltip::SentRecipientProtonMailPgpRecipient,
        },
    );
}

#[test]
fn sent_lock_icon_recipient_green_open_lock_with_pen_compose() {
    perform_sent_recipient_lock_icon_test(
        XPmContentEncryption::EndToEnd,
        XPmRecipientEncryption::None,
        XPmRecipientAuthentication::PgpInline,
        UiLock {
            icon: LockIcon::OpenLockWithPen,
            color: LockColor::Green,
            tooltip: LockTooltip::SentRecipientPGPSigned,
        },
    );
}

// --- Utility functions ---

#[allow(clippy::fn_params_excessive_bools)]
fn create_send_prefs<P>(
    pgp: &P,
    encrypt: bool,
    sign: bool,
    pgp_scheme: PackageCryptoType,
    mime_type: PackageMimeType,
    is_selected_key_pinned: bool,
    encryption_disabled: bool,
) -> SendPreferences<P::PublicKey>
where
    P: PGPProviderSync,
{
    let dummy_key = pgp
        .public_key_import(common::RECIPIENT_ONE, DataEncoding::Armor)
        .unwrap();
    SendPreferences {
        encrypt,
        sign,
        pgp_scheme,
        mime_type,
        selected_key: Some(dummy_key),
        is_selected_key_pinned,
        encryption_disabled,
        key_transparency_verification: Err(VerificationError::Unverified),
    }
}

fn perform_composer_lock_icon_test<Pub>(
    send_prefs: &SendPreferences<Pub>,
    expected_output: Option<UiLock>,
) where
    Pub: PublicKey,
{
    let output = UiLock::for_composer(send_prefs);
    assert_eq!(
        output, expected_output,
        "Expected {expected_output:?}, got {output:?}"
    );
}

struct RecipientLockIconTestParams<Pub: PublicKey> {
    origin_header: XPmOrigin,
    content_encryption_header: XPmContentEncryption,
    message_verification_status: MailVerificationStatus,
    verification_preferences_opt: Option<InboxVerificationPreferences<Pub>>,
}

fn recipient_lock_icon_test_params<P>(
    pgp: &P,
    origin_header: XPmOrigin,
    content_encryption_header: XPmContentEncryption,
    message_verification_status: MailVerificationStatus,
    pinned_key: bool,
) -> RecipientLockIconTestParams<P::PublicKey>
where
    P: PGPProviderSync,
{
    let dummy_key = pgp
        .public_key_import(common::RECIPIENT_ONE, DataEncoding::Armor)
        .unwrap();
    let verificiation_preferences = if pinned_key {
        InboxVerificationPreferences {
            ownership: KeyOwnership::Other,
            pinned_keys: vec![dummy_key.clone()],
            api_keys: vec![dummy_key],
            compromised_fingerprints: HashSet::default(),
            key_transparency_verification: Err(VerificationError::Unverified),
        }
    } else {
        InboxVerificationPreferences {
            ownership: KeyOwnership::Other,
            pinned_keys: vec![],
            api_keys: vec![dummy_key],
            compromised_fingerprints: HashSet::default(),
            key_transparency_verification: Err(VerificationError::Unverified),
        }
    };
    RecipientLockIconTestParams {
        origin_header,
        content_encryption_header,
        message_verification_status,
        verification_preferences_opt: Some(verificiation_preferences),
    }
}

fn perform_recipient_lock_icon_test<Pub>(
    params: &RecipientLockIconTestParams<Pub>,
    expected_output: UiLock,
) where
    Pub: PublicKey,
{
    let lock_icon = UiLock::for_receive_inbox(
        params.origin_header,
        params.content_encryption_header,
        params.message_verification_status,
        params.verification_preferences_opt.as_ref(),
    );
    assert_eq!(
        lock_icon, expected_output,
        "Expected {expected_output:?}, got {lock_icon:?}"
    );
}

fn perform_sent_lock_icon_test(
    origin_header: XPmOrigin,
    content_encryption: XPmContentEncryption,
    per_recipient_encryption: &[XPmRecipientEncryption],
    expected_output: UiLock,
) {
    let lock_icon =
        UiLock::for_sent_inbox(origin_header, content_encryption, per_recipient_encryption);
    assert_eq!(
        lock_icon, expected_output,
        "Expected {expected_output:?}, got {lock_icon:?}"
    );
}

fn perform_sent_recipient_lock_icon_test(
    content_encryption: XPmContentEncryption,
    recipient_encryption: XPmRecipientEncryption,
    recipient_authentication: XPmRecipientAuthentication,
    expected_output: UiLock,
) {
    let lock_icon = UiLock::for_sent_inbox_per_recipient(
        content_encryption,
        recipient_encryption,
        recipient_authentication,
    );
    assert_eq!(
        lock_icon, expected_output,
        "Expected {expected_output:?}, got {lock_icon:?}"
    );
}
