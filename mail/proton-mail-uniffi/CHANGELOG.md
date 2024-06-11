# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [Unreleased] - 2024-00-00

### Changed

- Message conversation id is no longer optional.
- The following functions now download their respective content if not available
    - `MailUserSession::conversation_with_remote_id`
    - `MailUserSession::conversation_with_id_and_context`
    - `MailUserSession::conversation_with_id_with_all_mail_context`
    - `MailUserSession::message_metadata_with_remote_id`

## [0.10.26] - 2024-06-10

### Changed

- Always return message id to open in when retrieving conversation messages.