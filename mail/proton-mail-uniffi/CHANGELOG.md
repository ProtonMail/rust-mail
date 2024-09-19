# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [unreleased] - 2024-00-00

### Added

- Expose `CoreAccount` and related types
- Add methods to query an account's login state
- Enable a partially completed login flow to be resumed

### Changed

- Split `StoredSession` into `StoredAccount` / `StoredSession`

## [0.11.49] - 2024-09-20

### Fixed

  - Do not trigger callbacks for synced pages

## [0.11.48] - 2024-09-19

### Fixed

  - Crash in paginator.reload()

## [0.11.47] - 2024-09-19

### Fixed

  - Message and Conversation pagination counters and element scroll

## [0.11.46] - 2024-09-18

### Changed

  - Removed first argument (`mail_settings`) from `MailUserSession::image_for_sender`.
  - `MailUserSession::initialize` now fetches data in parallel.

## [0.11.45] - 2024-09-18

### Added

  - Added the following APIs: `delete_messages`, `mark_messages_read`, `mark_messages_unread`.
  - Added `reload()` method to the message and conversation paginators

### Fixed

  - Custom folders order are no more random.
  - The CSS for the html mails has been patched

### Changed

  - `paginate_conversations_for_label` and `paginate_message_for_label` now sync data from the
     server.
  - `Mailbox` syncs 50 elements to match pagination behavior.

## [0.11.44] - 2024-09-12

### Changed

  - Errors for login flow are now returned as contextual Enums
  - `mark_conversations_as_unread` now expects a `Mailbox` rather than `MailUserSession`
  - Renamed `available_actions_for_conversation` to `available_actions_for_conversations`
    - Paramters changed from Id of the conversation to Vec<Id> of conversations and include view - Id of the Label
  - Renamed `available_actions_for_message` to `available_actions_for_messages`
    - Paramters changed from Id of the message to Vec<Id> of messages and include view - Id of the Label
  - Reimagined `ConversationAvailableAction` to contain each actions section representing final view.
    - renamed to `ConversationAvailableActions`
  - Reimagined `MessageAvailableAction` to contain each actions section representing final view.
    - renamed to `MessageAvailableActions`
  - Logs now always contain debug info, except for database debug logs.
  - When `MailSessionParams::log_debug` is set to true, database debug logs are also included.

### Added

  - `available_label_as_actions_for_messages` & `available_label_as_actions_for_conversations` methods exposing label_as actions
  - `available_move_to_actions_for_messages` & `available_move_to_actions_for_conversations` methods exposing move_to actions
  - `GeneralActions` enum representing static actions on message handled by FE.
  - `ReplyActions` enum respresenting reply options.
  - `IsSelected` enum representing selection state for Move & LabelAs actions.
  - `MoveAction` enum representing folder (either system or custom) to which item can be moved to.
  - `LabelAsAction` enum representing user applicable labels

## [0.11.43] - 2024-09-12

### Fixed

  - Pagination query errors

## [0.11.42] - 2024-09-12

### Fixed

  - Pagination query errors

## [0.11.41] - 2024-09-12

### Added

  - `paginate_conversations_for_label`.

### Fixed

  - Message display order in message views.
  - Message pagination query

## [0.11.40] - 2024-09-12

### Added

  - Added `total_messages` and `total_unread` to `Conversation`

### Fixed

  - HTML formatting

## [0.11.39] - 2024-09-11

### Fixed

  - Missing pagination exports

## [0.11.38] - 2024-09-11

### Added

  - Added `paginate_messages_for_label`.

### Changed

  - `ExclusiveLocation` enum now instead of listing all system exclusive locations, wraps them
    in `System { name: SystemLabel, id: Id }`

## [0.11.37] - 2024-09-09

### Fixed

- Login should now fail on wrong password

## [0.11.36] - 2024-09-09

### Fixed

  - Result types were not exported as enum.

### Changed

  - Update uniffi to v0.28.1

## [0.11.35] - 2024-09-09

### Fixed

  - Database locked error

### Changed

  - Errors for login flow are now returned as contextual Enums
  - `mark_conversations_as_unread` now expects a `Mailbox` rather than `MailUserSession`

## [0.11.34] - 2024-09-05

### Fixed

  - Query error in `Mailbox.watchUnreadCount`.

## [0.11.33] - 2024-09-03

### Fixed

  - Error when opening message body multiple times

## [0.11.32] - 2024-09-03

### Fixed

  - Fix callback leak in `wath_conversation_for_label`

## [0.11.31] - 2024-09-03

### Fixed

  - Fix callback leak in `wath_conversation`

## [0.11.30] - 2024-09-03

### Fixed

  - Query error in `ContextualConversation::watch_conversation_and_messages`

## [0.11.29] - 2024-09-03

### Added

  - Add `watch_message` to watch a single message.
  - Multi-account support added with session state management.
    - `MailSession::watch_stored_sessions()`
    - `MailSession::stored_session_states()`
    - `MailSession::watch_stored_session_states()`

### Fixed

  - Reduced how often callbacks get called.
  - Use `GET` for fetching messages.

## [0.11.28] - 2024-09-03

### Fixed

  - Add attachment file name to cached attachment.

## [0.11.27] - 2024-09-02

### Added

  - `apply_label_to_messages` who apply a label to many messages.
  - `remove_label_from_messages` who remove a label from many messages.

### Fixed

  - Fixed crash in `watchConversation`

## [0.11.26] - 2024-09-02

### Fixed

  - Fork with `web-account-lite` as version argument.
  - Excessive transactions in event loop.

### Changed

  - `conversation` now returns the conversation and the messages.
  - `conversation` may return null if the conversation is not found.
  - `watch_conversation` now only returns on handle.
  - `watch_conversation` may return null if the conversation is not found.
  - `conversation` and `watch_conversation` now sync the conversation's messages at least once.

## [0.11.25] - 2024-08-30

### Fixed

  - Fixed some notifications not being tracked.

## [0.11.24] - 2024-08-29

### Changed

  - `image_for_sender` now return a String who is a path to the image.

## [0.11.23] - 2024-08-29

### Changed

 - [iOS] library renamed to `proton_app_uniffi`

## [0.11.22] - 2024-08-28

### Added

  - Added new function `DecryptedMessage::get_multipart_data` that clients have to use to check if the message is multipart and they should show attachments.

### Changed

  - Rework `message_id_to_open` from `Option<Id>` to `Id` on `WatchedConversation` type
  - Removed callback from `MailUserSession::poll_events`

### Removed

 - `EventCallback` type.

## [0.11.21] - 2024-08-28

### Added

 - New callback_interface `EventCallback`.

### Fixed

 - Sync issues on multiple login.

### Changed

 - `StoredSession` `email` and `name` have been replaced with `name_or_address`.
 - `MailUserSession::poll_events` method now require callback_interface `EventCallback`.
 - Changed the following methods to be sync
    * `MailSession::create`
    * `MailSession::user_context_from_session`
    * `MailSession::stored_sessions`

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
