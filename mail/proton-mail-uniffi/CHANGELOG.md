# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.11.20] - 2024-08-27

### Changed

 - Renamed `Mailbox::load_attachment_to_buffer` to `Mailbox::get_attachment`.

## [0.11.19] - 2024-08-26

### Added

  - `Sidebar::all_custom_folders` method (return all custom folders in a flat way).

### Changed

  - Split `ContextualLabel` in `SidebarCustomFolder`, `SidebarCustomLabel` and `SidebarSystemLabel`.

### Fixed

  - DB query error in `watch_messages_for_label`.

## [0.11.18] - 2024-08-26

### Fixed

  - DB query error in `watch_conversations_for_label`.

## [0.11.17] - 2024-08-26

### Added

  - `CustomFolder` struct (like `ContextualLabel` with a children field).

### Fixed

  - DB query error in `watch_conversation_for_label`.

### Changed

  - `Sidebar::custom_folders` to return all custom_folders in a hierarchical way.

## [0.11.16] - 2024-08-23

### Fixed

  - Mail functions which accepted `MailSession` have been updated to `MailUserSession` instead.

## [0.11.15] - 2024-08-23

### Added

  - Added `avatar` property to `Conversation` and `Message` types.
  - Added `Mailbox::unread_count` and `Mailbox::watch_unread_count`.


## [0.11.14] - 2024-08-22

### Changed

  - Grouped `total_conv` and `total_msg` from `Label` as `total`
  - Grouped `unread_conv` and `unread_msg` from `Label` as `unread`

### Removed

  - Removed `initialized_conv` and `initialized_msg` from `Label`

## [0.11.13] - 2024-08-21

### Added

  - Added many stats about transformations to `BodyOutput`

### Fixed

  - Network request serialization/deserialization

### Removed

  - Removed `headers` from the `Message` type, see `DecryptedMessage instead`.
  - Removed `MessageBodyMetadata` and `EncryptedMessage` types.

## [0.11.12] - 2024-08-21

### Added

  - Added `Id` type to represent local IDs.
  - Added `Address.local_id` field.

### Changed

  - Changed all local IDs from `u64` to `Id`. This affects structs, functions,
    and errors.
  - Removed `local_` prefix from all ID fields:
      - `local_id` is now just `id`,
      - `local_conversation_id` is now just `conversation_id`,
      - `local_message_id` is now just `message_id`,
      - `local_parent_id` is now just `parent_id`,
      - Etc.
  - Changed grouping field types:
      - For `Contact`:
          - `label_ids` is now `Vec<LabelId>` instead of `Labels`.
      - For `ContactEmail`:
          - `contact_type` is now `Vec<String>` instead of `ContactTypes`.
          - `label_ids` is now `Vec<LabelId>` instead of `Labels`.
      - For `Conversation`:
          - `recipients` is now `Vec<MessageAddress>` instead of
            `MessageAddresses`.
          - `senders` is now `Vec<MessageAddress>` instead of
            `MessageAddresses`.
      - For `Message`:
          - `bcc_list` is now `Vec<MessageAddress>` instead of
            `MessageAddresses`.
          - `cc_list` is now `Vec<MessageAddress>` instead of
            `MessageAddresses`.
          - `parsed_headers` is now `HashMap<String, String>` instead of
            `ParsedHeaders`.
          - `reply_tos` is now `Vec<MessageAddress>` instead of
            `MessageAddresses`.
          - `to_list` is now `Vec<MessageAddress>` instead of
            `MessageAddresses`.
      - For `MessageBodyMetadata`:
          - `parsed_headers` is now `HashMap<String, String>` instead of
            `ParsedHeaders`.
  - Renamed `Mailbox::with_local_id()` to `with_label_id()`.
  - Changed `Labels` color now take in account the `MailSettings`.

### Fixed

  - Missing async wrapper for `Mailbox::inbox()`.

### Removed

  - Removed grouping types:
      - `ContactTypes`,
      - `Labels`,
      - `MessageAddresses`,
      - `MessageAttachmentInfos` (was unused),
      - `MessageAttachments` (was unused),
      - `ParsedHeaders`.
  - Removed fields:
      - `Message.deleted`,
      - `Message.mime_type`.
  - Removed `LabelId`, as `Id` is now being used for all local IDs. Affects:
      - `Contact.label_ids`,
      - `ContactEmail.label_ids`,
      - `ConversationCount.label_ids`,
      - `MessageCount.label_ids`.
  - Removed `MailboxError::RemoteLabelNotFound` variant.
  - Removed `Mailbox.with_remote_id()`.
  - Removed `RemoteId` type.

## [0.11.11] - 2024-08-20

### Added

  - Added `message_id_to_open` field into `WatchedConversation`

### Changed

  - Updated `ConversationSearchOptions` to use local IDs.
  - Updated `MessageSearchOptions` to use local IDs.
  - Renamed `MessageSearchOptions.label_id` to `label_ids`.
  - Changed `Message.address_id` to use a local ID.

### Removed

  - Removed `load_remote_conversation()` as it is no longer usable with the
    removal of remote IDs.
  - Removed `Label.remote_parent_id` and `Message.remote_conversation_id`.
  - Removed `RemoteIds` as it is no longer used/needed.

## [0.11.10] - 2024-08-20

### Added

  - Added `MimeType` for attachment,
      - Added `category` field to determin media icon for attachment
  - Added `SystemLabel` enum available on `Label.label_description` field.
  - Added `LabelDescription` enum
      - `LabelDescription` enum contains `System` field with optional `SystemLabel` information

### Changed

  - Replaced `LabelType` enum with `LabelDescription` enum on the `Label` type.
  - Added `mime_type` in `DecryptedMessage`

### Fixed

  - Error storing credentials after login.

## [0.11.9] - 2024-08-19

### Fixed

  - Execute all exported functions on our own async runtime.

## [0.11.8] - 2024-08-19

### Added

  - Added watching of `Conversation` as well as its messages
      - Added `conversation` and extra handle to `WatchedConversation`.
      - Added watching of `Conversation` as well as its `Messages` to
        `watch_conversation()`.
  - Added watchers for conversations and messages by label
      - Added `watch_conversations_for_label()` and
        `watch_messages_for_label()`.
  - Added methods to get messages and conversations
      - Added `conversation()` and `conversations_for_label()`.
      - Added `message()`, `messages_for_conversation()`, and
        `messages_for_label()`.
  - Added getter and watcher for MailSettings
    - Added `mail_settings()`
    - Added `watch_mail_settings()`

### Changed

  - Renamed exported UniFFI conversation methods to be long-form
      - `apply_label()` -> `apply_label_to_conversations()`
      - `delete()` -> `delete_conversations()`
      - `load()` -> `load_conversation()`
      - `load_remote()` -> `load_remote_conversation()`
      - `mark_as_read()` -> `mark_conversations_as_read()`
      - `mark_as_unread()` -> `mark_conversations_as_unread()`
      - `relocate()` -> `move_conversations()`
      - `remove_label()` -> `remove_label_from_conversations()`
      - `star()` -> `star_conversations()`
      - `unstar()` -> `unstar_conversations()`
      - `watch()` -> `watch_conversation()`
  - Corrected `starred` field to be `is_starred`.

## [0.11.7] - 2024-08-16

### Added

  - Added `available_actions_for_message()` and
    `available_actions_for_conversation()`.

### Fixed

  - Fixed login error related to `no user id set`.

### Changed

  - Grouped `watch_standard_labels()`, `watch_folder_labels()` and
    `watch_system_labels()` into `watch_labels()`.
  - `RemoteId` removed from exported types
  - `LocalId`s are non-optional

## [0.11.6] - 2024-08-13

### Changed

  - Added `custom_labels` and `starred` properties to `Conversation` and
    `Message`.
  - Added `time` property to `Conversation`.
  - Methods that load conversation now require the local label id for the
    location they are meant to be displayed in.
  - Added more know system labels to database initialization.
  - Added callbacks `watch_standard_labels()`, `watch_folder_labels()` and
    `watch_system_labels()` to `Sidebar`.

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
