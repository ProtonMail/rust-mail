# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

**As of January 3, 2025, this project has moved to a monorepo, and the changelog will no longer be maintained**

## [Unreleased] - 2024-00-00

## [0.9.0] - 2024-12-17

### Added
- Make API PQC ready with OpenPGP v6 keys support.
- Encryption preferences consider v6 public keys.

### Changed

- Refactor attachment encryption API allowing encryption with the `PrimaryAddressKey` only.
- Refactor draft encryption API allowing encryption with the `PrimaryAddressKey` only.
- Update `proton-crypto-account` to 0.8.0

## [0.8.2] - 2024-12-13

### Changed

- Update `proton-crypto-account` to 0.7.4 and `proton-crypto-inbox-mime` to 0.4.1

## [0.8.1] - 2024-11-26

### Changed

- Update `proton-crypto-account` to 0.7.3

## [0.8.0] - 2024-11-22

### Changed

- Make types serializable for email sending.
- Update `proton-crypto-account` to 0.7.2 and `proton-crypto-inbox-mime` to 0.4.0

## [0.7.3] - 2024-11-19

### Changed

- Update `proton-crypto-account` to 0.7.1 and `proton-crypto-inbox-mime` to 0.3.3

## [0.7.2] - 2024-10-30

### Changed

- Update `proton-crypto-inbox-mime` to 0.3.2

## [0.7.1] - 2024-10-22

### Changed

- Update `proton-crypto-account` to 0.7.0 and `proton-crypto-inbox-mime` to 0.3.1

## [0.7.0] - 2024-10-02

### Added

- Add `encrypt_session_key_to_recipient` method on `ExtractedAttachmentInfo`.
- Add `new_with_draft` method on `EncryptedPackageBody`.
- End-to-end integration tests for email sending.

### Changed

- Unified use of `EncryptedMessageBody` and `InboxSessionKey` types across the crate.
- Update `proton-crypto-account` to 0.6.3 and `proton-crypto-inbox-mime` to 0.3.0.

## [0.6.5] - 2024-09-11

### Dependencies

- update proton-crypto-account to 0.6.2 and proton-crypto-inbox-mime to 0.2.5

## [0.6.4] - 2024-08-27

### Added

- Re-exported mime crate.

## [0.6.3] - 2024-08-26

### Added

- `EncryptionPreferences`: Introduced a new type that consolidates data from contact/API keys and user mail settings. This type streamlines the preparation process for encrypting data intended for a recipient.

- `SendPreferences`: Introduced a new type that aggregates information from contact/API keys and user mail settings. This type facilitates the preparation of sending an email to a recipient by specifying details related to signing, encryption, and the keys to be used.

## [0.6.2] - 2024-08-13

### Dependencies

- update proton-crypto-account to 0.6.0 and proton-crypto-inbox-mime to 0.2.4

## [0.6.1] - 2024-07-31

### Dependencies

- update proton-crypto-account to 0.5.1 and proton-crypto-inbox-mime to 0.2.3

## [0.6.0] - 2024-07-30

### Changed

- Change optional verification_context type to `Option<&Prov::VerificationContext>`.

## [0.5.1] - 2024-07-23

### Maintenance

- Updating to new proton-crypto-account version

## [0.5.0] - 2024-07-22

### Added

- Logic to encrypt and sign packages for sending emails (#86)
- Logic to extract the session key and data packet from a PGP message (#85)
- New trait for GettablePGPMessages used by DecryptableMessages and SessionKeyAndDataPacketsExtractable (#85)
- Utility to re-encrypt attachments to new recipients (#88)
- Attachment encryption functions are now packaged under a new EncryptableAttachment trait (#83)
- AttachmentDecryption trait renamed to DecryptableAttachment (#83)
- Draft encryption function now returns a new EncryptedDraft string_id! type (#83)

## [0.4.2] - 2024-06-26


