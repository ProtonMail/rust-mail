# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] - 2024-00-00

### Added

- `LabelAs` action for conversations.
- Available actions for messages in bottom bar.
- `LabelAs` action for messages.
- `Move` actions for messages.
- `Label` and `Unlabel` actions for messages.
- The following actions for the messages:
    - `Label`: `apply_label_to_messages`
    - `Unlabel`: `remove_label_from_messages`
    - `Delete`: `delete_messages`
    - `Read`: `mark_messages_read`
    - `Unread`: `mark_messages_unread`
- `CustomFolder` struct (like `ContextualLabel` with a children field)
- `Sidebar::all_custom_folders` method (return all custom folders in a flat way)
- Added a background job for expired messages.
- New error targeting end users (Login flow only)
- `MessageBody` and `Attachment` cache persistence.
- Expose `CoreAccount` and related types
- Add methods to query an account's login state
- Enable a partially completed login flow to be resumed

### Changed

- Cache key for attachments into (u64, String)
- Split `ContextualLabel` in `CustomFolder`, `CustomLabel` and `SystemLabel`.
- `Sidebar::custom_folders` to return all custom_folders in a hierarchical way.
- Grouped `total_conv` and `total_msg` from `ContextualLabel` as `total`.
- Grouped `unread_conv` and `unread_msg` from `ContextualLabel` as `unread`.
- Label parent is now resolved at load time.
- Attachment and MessageBodies cache now use `get_path_or_insert`.
- Removed first argument (`mail_settings`) from `MailUserContext::image_for_sender`.
- Split `EncryptedUserSession` into `CoreAccount` / `CoreSession`

### Fixed

- `MessageBodyMetadata` local_id is the same as the one from `Message`
- Custom folders order are no more random.

### Removed

- Removed `initialized_conv` and `initialized_msg` from `ContextualLabel`


## [0.5.31] - 2024-07-22

### Changed

- `DecryptedMessage` now reads mail settings from the database.

## [0.5.30] - 2024-07-19

### Added

- Disable/Enable remote image in HTML content in `DecryptedMessageBody`.

### Changed

- Renamed `DecryptedMessageBody` into `DecryptedMessage`.
- Renamed `DecryptedMessageBodyError` into `DecryptedMessageError`.

## [0.5.29] - 2024-07-15

### Added

- Strip UTM parameters from HTML content.
- [iOS] Inject viewport metadata for web view.

## [0.5.28] - 2024-07-02

### Fixed

- Ensure mail cache path is unique per user.
- Fix conversation message selection.
- Use Get with query parameters for message metadata.

## [0.5.27] - 2024-06-21

### Changed

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
