# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] - 2024-00-00

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


