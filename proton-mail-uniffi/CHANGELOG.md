# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] - 2024-00-00

### Added

- `available_actions_for_message` and `available_actions_for_conversation`.

### Fixed

- Fixed login error related to `no user id set`.

### Changed

- Grouped  `watch_standard_labels`, `watch_folder_labels` and `watch_system_labels` into `watch_labels`.
- RemoteId removed from exported types
- LocalIds are non-optional

## [0.11.6] - 2024-08-13

### Changed

- Added `custom_labels`  and  `starred` properties to `Conversation`
  and `Message`.
- Added `time` property to `Conversation`.
- Methods that load conversation now require the local label id for the
  location they are meant to be displayed in.
- Added more know system labels to database initialization.
- Added callbacks `watch_standard_labels`, `watch_folder_labels` and `watch_system_labels` to `Sidebar`.

## [0.11.5] - 2024-08-09

### Added

- Added `Sidebar` type to represent the sidebar.

### Changed

- Attachments, message bodies and sender images are now stored in the cache

## [0.11.4] - 2024-08-08

### Added

- Added new live query interface
    - `conversations::watch()` (replacement for
      `Mailbox::new_conversation_messages_live_query()`)
    - `labels::watch_folder_labels()` (replacement for
      `MailUserSession::new_folder_labels_observed_query()`)
    - `labels::watch_standard_labels()` (replacement for
      `MailUserSession::new_label_labels_observed_query()`)
    - `labels::watch_system_labels()` (replacement for
      `MailUserSession::new_system_labels_observed_query()`)

## [0.11.2] - 2024-08-07

### Fixed

- Session login and storage.

## [0.11.1] - 2024-08-05

- Internal tag update.

## [0.11.0] - 2024-08-05

### Added

- `datatypes` have been added for a couple of reasons - first, to provide a
  common basis for all types that get exposed to UniFFI; and second, to
  provide clarity and further centralised information about the types. Note
  that not all are used at present, but this forms the basis of the
  translation layer of the facade.

### Removed

- The live query mechanism has changed, and the following have been removed:
    - `MailUserSession::new_folder_labels_observed_query()`
    - `MailUserSession::new_label_labels_observed_query()`
    - `MailUserSession::new_system_labels_observed_query()`
    - `Mailbox::new_conversation_live_query()`
    - `Mailbox::new_conversation_messages_live_query()`
    - `Mailbox::new_item_live_query()`
    - `Mailbox::new_message_live_query()`

- The following have been removed as they are no longer necessary:
    - `MailUserSession::message_metadata()`
    - `MailUserSession::message_metadata_with_remote_id()`

### Changed

- Moved and renamed in `MailUserSession`:
    - `filter_conversations()` ->
      `mail::conversations::search_for_conversations()` (temporary - will
      become `mail::messages::conversations()` later)
    - `conversation_with_remote_id()` -> `mail::conversations::load_remote()`
    - `conversation_with_id_and_context()` -> `mail::conversations::load()`
    - `conversation_with_id_with_all_mail_context()` ->
      `mail::conversations::load()`
    - `filter_messages()` -> `mail::messages::search_for_messages()`
      (temporary - will become `mail::messages::search()` later)

- Moved and renamed in `Mailbox`:
    - `delete_conversations()` -> `mail::conversations::delete()`
    - `label_conversations()` -> `mail::conversations::apply_label()`
    - `mark_conversations_read()` -> `mail::conversations::mark_as_read()`
    - `mark_conversations_unread()` -> `mail::conversations::mark_as_unread()`
    - `message_body()` -> `mail::messages::body()`
    - `move_conversations()` -> `mail::conversations::relocate()`
    - `move_conversations_with_remote_id()` ->
      `mail::conversations::relocate()`
    - `star_conversations()` -> `mail::conversations::star()`
    - `unlabel_conversations()` -> `mail::conversations::remove_label()`
    - `unstar_conversations()` -> `mail::conversations::unstar()`

## [0.10.34] - 2024-07-22

### Changed

- `DecryptedMessage` now reads mail settings from the database.

## [0.10.33] - 2024-07-19

### Added

- The following functions have been exposed:
    - `avatar_information_from_name_and_email()`
    - `avatar_information_from_message_addresses()`
    - `avatar_information_from_message_address()`
- Disable/Enable remote image in HTML content in `DecryptedMessageBody`.
- Added `rust_log_*` functions to log into the rust log.

### Changed

- Renamed `DecryptedMessageBody` to `DecryptedMessage`.

## [0.10.32] - 2024-07-15

### Added

- Strip UTM parameters from HTML content
- [iOS] Inject viewport metadata for web view

## [0.10.31] - 2024-07-02

### Fixed

- Ensure mail cache path is unique per user.
- Fixed conversation message selection.
- Use Get with query parameters for message metadata.

## [0.10.30] - 2024-06-21

## Changed

- Updated `MailUserSession::filter_converstions` to require a label id for
  context.

## [0.10.29] - 2024-06-21

### Changed

- Adds additional debug logs.

## [0.10.28] - 2024-06-18

### Fixed

- Correctly initialize address id in attachments when created from message
  data.

## [0.10.27] - 2024-06-12

### Changed

- Message conversation id is no longer optional.
- The following functions now download their respective content if not
  available:
    - `MailUserSession::conversation_with_remote_id()`
    - `MailUserSession::conversation_with_id_and_context()`
    - `MailUserSession::conversation_with_id_with_all_mail_context()`
    - `MailUserSession::message_metadata_with_remote_id()`

## [0.10.26] - 2024-06-10

### Changed

- Always return message id to open in when retrieving conversation messages.
