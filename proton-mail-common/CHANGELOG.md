# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] - 2024-00-00

## [0.5.27] - 2024-06-21

## Changed

- Update `MailUserContext::filter_converstions` to require a label id for context.

## [0.5.26] - 2024-06-21

### Changed

- Adds additional debug logs.

## [0.5.25] - 2024-06-18

### Fixed

- Correctly initialize address id in attachments when created from message data.

## [0.5.24] - 2024-06-12

### Changed

- Message conversation id is no longer optional.
- The following functions now download their respective content if not available
    - `MailUserContext::conversation_with_remote_id`
    - `MailUserContext::conversation_with_id_and_context`
    - `MailUserContext::conversation_with_id_with_all_mail_context`
    - `MailUserContext::message_metadata_with_remote_id`

## [0.5.23] - 2024-06-10

### Changed

- Always return message id to open in when retrieving conversation messages.