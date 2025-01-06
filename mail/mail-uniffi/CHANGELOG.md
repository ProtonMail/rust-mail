# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [unreleased] - 2024-00-00

### Changed 

  - `DecryptedMessage::body` is now infailable.
  - `TranformOpts` has been changed to contain exclusively `bool` and `Option<bool>`
  - New helper method `DecryptedMessage::body_with_defaults` with the default options for the user.

#### BodyOutput changes

  - It now returns the used `TransformOpts`.
  - It returns the `BodyBanners` that should be displayed to the user.
  - More stats:
    - `remote_images_disabled`
    - `embedded_images_disabled`
    - `images_proxied`


### Removed

  - `RemoteContent` enum
  - `BlockQuote` enum

### Added

  - Disable embedded images pass and toggle.

### Fix

  - Mark unread action
  - Significant performance improvements to the body transformation process.
  - Fixed a bug where not all images got proxied in the presence of embedded images.
  - Remote images are properly loaded.


## [0.31.5] - 2024-12-23

### Added

  - Log errors send to UI

### Fix

  - Properly handle address events

## [0.31.4] - 2024-12-20

### Fix

  - Fix muon error mapping conversion.

## [0.31.3] - 2024-12-20

### Fix

  - Fix expand label response.

## [0.31.2] - 2024-12-20

### Fix

  - Fix missing registration for certain actions.
  - Unregistered actions are not allowed to execute in the queue.

## [0.31.1] - 2024-12-20

### Fix

  - Remove unnecessary sync on Mailbox during initiation.

## [0.31.0] - 2024-12-19

### Changed

  - Event loop and queue can now safely be called concurrently.

### Fix

  - Unrelated callbacks in table observers

## [0.30.1] - 2024-12-19

### Fix

  - Restore early exit table watchers.
  - Fix missing message/conversation counters from event loop update.

## [0.30.0] - 2024-12-19

### Changed

  - New notification system

## [0.29.0] - 2024-12-18

### Changed

  - Use `muon` instead of `reqwest` for all API communication

## [0.28.1] - 2024-12-18

### Fixed

  - Bcc & Cc recipients going missing in draft creation.

### Changed

## [0.28.0] - 2024-12-18

 - Opening a Draft now always syncs the contents from the server.
 - SQL debug logs are only enabled when `STASH_SQL_DEBUG` environment variable is present.

## [0.27.0] - 2024-12-17

### Added

  - Added `Draft::message_id`.

### Fixed

  - Message marked a read are no more displayed when coming back in conversation list while filtering read.
  - Embedded attachments are faster
  - Fixed some bugs regarding embedded attachments not showing.

### Changed

  - `get_embedded_attachment` must be called from `DecryptedMessageBody`

## [0.26.0] - 2024-12-13

### Added

  - Draft recipient validation.

### Changed

  - Fixed some bugs regarding embedded attachments not showing.
  - Mutating the `Draft` now auto triggers save.

### Fixed

  - Queued actions not executing.

## [0.25.0] - 2024-12-13

### Added

  - Failable methods & functions no longer throw errors but rather encapsulate in distinct Result type for each method.
    - Eg. `conversation -> Result<Conversation, MailboxError>` will now return `conversation -> ConversationResult` where
      ```rust
        enum ConversationResult {
          Ok(Conversation),
          Error(ProtonMailError)
        }
      ```
  - `ProtonMailError` struct which is the new error interface returned by all failables. It contains
    - `MailErrorKind` which describe source of the error such as eg. `UserActionError`
    - `MailErrorDetails` which categorize error into eg. `Network` errors or specific `Reason` of that error occurrence.

### Changed

  - `UserLoginFlowVoidResult` was replaced with `VoidProtonMailResult`
  - `LoginFlow::user_id()` method now return `LoginFlowUserIdResult` instead of `UserLoginFlowStringResult` which differ only by a name.
  - `LoginFlow::session_id()` method now return `LoginFlowSessionIdResult` instead of `UserLoginFlowStringResult` which differ only by a name.
  - `LoginFlow::to_user_context()` method now return `LoginFlowToUserContextResult` instead of `UserLoginFlowArcMailUserSessionResult` which differ only by a name.
  - `MailSession::new_login_flow()` method now return `MailSessionNewLoginFlowResult` instead of `UserLoginFlowArcLoginFlowResult` which differ only by a name.
  - `MailSession::resume_login_flow()` method now return `MailSessionResumeLoginFlowResult` instead of `UserLoginFlowArcLoginFlowResult` which differ only by a name.
  - `apply_label_to_conversations` function now returns `VoidProtonMailResult` instead of `Result<(), MailSessionError>`.
  - `delete_conversations` function now returns `VoidProtonMailResult` instead of `Result<(), MailSessionError>`.
  - `available_actions_for_conversations` function now returns `AvailableActionsForConversationsResult` instead of `Result<ConversationAvailableActions, MailboxError>`.
  - `available_label_as_actions_for_conversations` function now returns `AvailableLabelAsActionsForConversationsResult` instead of `MailboxResult<Vec<LabelAsAction>>`.
  - `available_move_to_actions_for_conversations` function now returns `AvailableMoveToActionsForConversationsResult` instead of `MailboxResult<Vec<MoveAction>>`.
  - `all_available_bottom_bar_actions_for_conversations` function now returns `AllAvailableBottomBarActionsForConversationsResult` instead of `MailboxResult<AllBottomBarMessageActions>`.
  - `conversation` function now returns `ConversationResult` instead of `Result<Option<ConversationAndMessages>, MailboxError>`.
  - `conversations_for_label` function now returns `ConversationsForLabelResult` instead of `Result<Vec<Conversation>, MailboxError>`.
  - `load_conversation` function now returns `LoadConversationResult` instead of `Result<Option<Conversation>, MailboxError>`.
  - `mark_conversations_as_read` function now returns `VoidProtonMailResult` instead of `Result<(), MailSessionError>`.
  - `mark_conversations_as_unread` function now returns `VoidProtonMailResult` instead of `Result<(), MailSessionError>`.
  - `move_conversations` function now returns `VoidProtonMailResult` instead of `Result<(), MailSessionError>`.
  - `paginate_conversations_for_label` function now returns `PaginateConversationsForLabelResult` instead of `Result<ConversationPaginator, MailboxError>`.
  - `remove_label_from_conversations` function now returns `VoidProtonMailResult` instead of `Result<(), MailSessionError>`.
  - `search_for_conversations` function now returns `SearchForConversationsResult` instead of `Result<Vec<Conversation>, MailSessionError>`.
  - `star_conversations` function now returns `VoidProtonMailResult` instead of `Result<(), MailSessionError>`.
  - `unstar_conversations` function now returns `VoidProtonMailResult` instead of `Result<(), MailSessionError>`.
  - `watch_conversation` function now returns `WatchConversationResult` instead of `Result<Option<WatchedConversation>, MailboxError>`.
  - `watch_conversations_for_label` function now returns `WatchConversationsForLabelResult` instead of `Result<WatchedConversations, MailboxError>`.
  - `label_conversations_as` function now returns `LabelConversationsAsResult` instead of `Result<bool, MailboxError>`.
  - `get_attachment` function now returns `GetAttachmentResult` instead of `Result<DecryptedAttachment, MailboxError>`.
  - `body` method of `DecryptedMessage` now returns `BodyResult` instead of `Result<BodyOutput, MailboxError>`.
  - `message` function now returns `MessageResult` instead of `Result<Option<Message>, MailboxError>`.
  - `watch_message` function now returns `WatchMessageResult` instead of `Result<Option<WatchedMessage>, MailboxError>`.
  - `messages_for_conversation` function now returns `MessagesForConversationResult` instead of `Result<Vec<Message>, MailboxError>`.
  - `messages_for_label` function now returns `MessagesForLabelResult` instead of `Result<Vec<Message>, MailboxError>`.
  - `paginate_messages_for_label` function now returns `PaginateMessagesForLabelResult` instead of `Result<MessagePaginator, MailboxError>`.
  - `paginate_search` function now returns `PaginateSearchResult` instead of `Result<MessagePaginator, MailboxError>`.
  - `search_for_messages` function now returns `SearchForMessagesResult` instead of `Result<Vec<Message>, MailSessionError>`.
  - `available_actions_for_messages` function now returns `AvailableActionsForMessagesResult` instead of `MailboxResult<MessageAvailableActions>`.
  - `available_label_as_actions_for_messages` function now returns `AvailableLabelAsActionsForMessagesResult` instead of `MailboxResult<Vec<LabelAsAction>>`.
  - `available_move_to_actions_for_messages` function now returns `AvailableMoveToActionsForMessagesResult` instead of `MailboxResult<Vec<MoveAction>>`.
  - `all_available_bottom_bar_actions_for_messages` function now returns `AllAvailableBottomBarActionsForMessagesResult` instead of `MailboxResult<AllBottomBarMessageActions>`.
  - `get_message_body` function now returns `GetMessageBodyResult` instead of `MailSessionResult<DecryptedMessage>`.
  - `watch_messages_for_label` function now returns `WatchMessagesForLabelResult` instead of `Result<WatchedMessages, MailboxError>`.
  - `apply_label_to_messages` function now returns `VoidProtonMailResult` instead of `Result<(), MailSessionError>`.
  - `star_messages` function now returns `VoidProtonMailResult` instead of `Result<(), MailSessionError>`.
  - `unstar_messages` function now returns `VoidProtonMailResult` instead of `Result<(), MailSessionError>`.
  - `remove_label_from_messages` function now returns `VoidProtonMailResult` instead of `Result<(), MailSessionError>`.
  - `mark_messages_read` function now returns `VoidProtonMailResult` instead of `Result<(), MailSessionError>`.
  - `mark_messages_unread` function now returns `VoidProtonMailResult` instead of `Result<(), MailSessionError>`.
  - `delete_messages` function now returns `VoidProtonMailResult` instead of `Result<(), MailSessionError>`.
  - `move_messages` function now returns `VoidProtonMailResult` instead of `Result<(), MailSessionError>`.
  - `label_messages_as` function now returns `LabelMessagesAsResult` instead of `Result<bool, MailSessionError>`.
  - `Sidebar::system_labels` method now returns `SidebarSystemLabelsResult` instead of `SidebarResult<Vec<SidebarSystemLabel>>`.
  - `Sidebar::custom_folders` method now returns `SidebarCustomFoldersResult` instead of `SidebarResult<Vec<SidebarCustomFolder>>`.
  - `Sidebar::all_custom_folders` method now returns `SidebarAllCustomFoldersResult` instead of `SidebarResult<Vec<SidebarCustomFolder>>`.
  - `Sidebar::custom_labels` method now returns `SidebarCustomLabelsResult` instead of `SidebarResult<Vec<SidebarCustomLabel>>`.
  - `Sidebar::collapse_folder` method now returns `VoidProtonMailResult` instead of `SidebarResult<()>`.
  - `Sidebar::expand_folder` method now returns `VoidProtonMailResult` instead of `SidebarResult<()>`.
  - `Mailbox::new()` method is now a function `new_mailbox` and returns `NewMailboxResult` instead of `MailboxResult<Arc<Self>>`.
  - `Mailbox::inbox()` method is now a function `new_inbox_mailbox` and returns `NewMailboxResult` instead of `MailboxResult<Arc<Self>>`.
  - `Mailbox::all_mail()` method is now a function `new_all_mail_mailbox` and returns `NewMailboxResult` instead of `MailboxResult<Arc<Self>>`.
  - `Mailbox::with_label_id()` method is now a function `with_label_id_mailbox` and returns `NewMailboxResult` instead of `MailboxResult<Arc<Self>>`.
  - `Mailbox::unread_count()` method now returns `MailboxUnreadCountResult` instead of `MailboxResult<u64>`.
  - `Mailbox::watch_unread_count()` method now returns `MailboxWatchUnreadCountResult` instead of `MailboxResult<Arc<WatchHandle>>`.
  - `MailSession::create()` method is now a function `create_mail_session` and returns `CreateMailSessionResult` instead of `MailSessionResult<Arc<Self>>`.
  - `MailSession::user_context_from_session()` method now returns `MailSessionUserContextFromSessionResult` instead of `MailSessionResult<Arc<MailUserSession>>`.
  - `MailSession::get_accounts()` method now returns `MailSessionGetAccountsResult` instead of `MailSessionResult<Vec<Arc<StoredAccount>>>`.
  - `MailSession::watch_accounts()` method now returns `MailSessionWatchAccountsResult` instead of `MailSessionResult<WatchedAccounts>`.
  - `MailSession::get_account()` method now returns `MailSessionGetAccountResult` instead of `MailSessionResult<Option<Arc<StoredAccount>>>`.
  - `MailSession::get_sessions()` method now returns `MailSessionGetSessionsResult` instead of `MailSessionResult<Vec<Arc<StoredSession>>>`.
  - `MailSession::watch_sessions()` method now returns `MailSessionWatchSessionsResult` instead of `MailSessionResult<WatchedSessions>`.
  - `MailSession::get_session()` method now returns `MailSessionGetSessionResult` instead of `MailSessionResult<Option<Arc<StoredSession>>>`.
  - `MailSession::get_account_state()` method now returns `MailSessionGetAccountStateResult` instead of `MailSessionResult<Option<StoredAccountState>>`.
  - `MailSession::get_session_state()` method now returns `MailSessionGetSessionStateResult` instead of `MailSessionResult<Option<StoredSessionState>>`.
  - `MailSession::get_primary_account()` method now returns `MailSessionGetPrimaryAccountResult` instead of `MailSessionResult<Option<Arc<StoredAccount>>>`.
  - `MailSession::set_primary_account()` method now returns `VoidProtonMailResult` instead of `MailSessionResult<()>`.
  - `MailSession::logout_account()` method now returns `VoidProtonMailResult` instead of `MailSessionResult<()>`.
  - `MailSession::delete_account()` method now returns `VoidProtonMailResult` instead of `MailSessionResult<()>`.
  - `MailUserSession::logout()` method now returns `VoidProtonMailResult` instead of `MailSessionResult<()>`.
  - `MailUserSession::fork()` method now returns `MailUserSessionForkResult` instead of `MailSessionResult<String>`.
  - `MailUserSession::user()` method now returns `MailUserSessionUserResult` instead of `MailSessionResult<User>`.
  - `MailUserSession::initialize()` method now returns `VoidProtonMailResult` instead of `MailSessionResult<()>`.
  - `MailUserSession::movable_folders()` method now returns `MailUserSessionMovableFoldersResult` instead of `MailSessionResult<Vec<SidebarCustomFolder>>`.
  - `MailUserSession::applicable_labels()` method now returns `MailUserSessionApplicableLabelsResult` instead of `MailSessionResult<Vec<SidebarCustomLabel>>`.
  - `MailUserSession::get_sender_image()` method now returns `MailUserSessionGetSenderImageResult` instead of `MailSessionResult<Option<String>>`.
  - `MailUserSession::execute_pending_action()` method now returns `VoidProtonMailResult` instead of `MailSessionResult<()>`.
  - `MailUserSession::execute_pending_actions()` method now returns `VoidProtonMailResult` instead of `MailSessionResult<()>`.
  - `watch_mail_settings()` function now returns `WatchMailSettingsResult` instead of `MailSessionResult<SettingsWatcher>`.
  - `Draft::new()` method is now a function `new_draft` and returns `NewDraftResult` instead of `MailSessionResult<Arc<Self>>`.
  - `Draft::open()` method is now a function `open_draft` and returns `NewDraftResult` instead of `MailSessionResult<Arc<Self>>`.


### Removed

  - Removed `MailSessionError` and `MailboxError` in favor of `UserSessionError` & `UserActionError`.

## [0.24.0] - 2024-12-11

### Changed

  - Log back trace on panic.
  - `MessageAddress` type has been split into `MessageSender`, `MessageRecipient`
     and `MessageReplyTo` types as they were incorrectly mapped.
     - This affect the `Conversation` and `Message` types.
  - `avatar_information_from_message_address` has been split into
    - `avatar_information_from_message_sender`
    - `avatar_information_from_message_recipient`
  - `avatar_information_from_message_addresses` has been split into
    - `avatar_information_from_message_senderes`
    - `avatar_information_from_message_recipients`

### Fix

  - Marking read (or unread) already read (or unread) messages or conversations is now no-op.


## [0.23.0] - 2024-11-28

### Changed

  - `MailUserSession` can now be created multiple times for a logged in session.
  - `MailUserSession` will fail with an error if you attempt to log in if an existing session is
    active.

### Fix

  - Conversations displayed as `read` with unread message in another mailbox now propose `mark as unread` action.

## [0.22.2] - 2024-11-28

### Fix

  -  Cids with the format `<foo@bar>` no longer need angle brackets.

## [0.22.1] - 2024-11-26

### Changed

  - `get_embedded_attachment` now triggers errors on unknown CIDs

## [0.22.0] - 2024-11-26

### Added

  - `Draft::send` to send drafts

### Fix

  - Properly show partial selection on conversations when a message does't have a label.
  - Now moving message work in message view mode too.

## [0.21.6] - 2024-11-22

### Fix

  - Embedded images no longer get proxied.

## [0.21.5] - 2024-11-20

### Fix

  - `markConversationAsRead` now mark conversation as Read
  - `markMessageAsRead` now mark message as Read
  - `LabelAs` now update only LabelType::Label
  - `WatchHandle` now properly disconnects when it's dropped.
  - Blocking code is now allowed in the watcher callbacks.
  - Double action execution for queued actions

## [0.21.4] - 2024-11-13

### Added

  - Removed InAppPromosHidden field from API
  - Added `watch_available_move_to_actions`.

## [0.21.3] - 2024-11-08

### Fix

  - Avatar display text was changed (back) to use only one letter
  - Improved grouping mechanism by using Avatar's display text
  - Shorten dampening period significantly
  - Move Conversation to trash or spam no longer fails with error


## [0.21.2] - 2024-11-07

### Fix

  - Change the `put_delete_contacts` response `Id` to `ID`


### Changed

  - `move_message` to remove `source_id` argument.

## [0.21.1] - 2024-11-07

### Fix

  - reenable contact events

## [0.21.0] - 2024-11-06

### Added

  - Exposed `delete_contact` functionality.
  - Exposed `watch_contact_list` with new `ContactsLiveQueryCallback` interface.
  - PGP attachments in message cache.

### Fix

  - Wrap blocking code of callback in `spawn_blocking`
  - Distribute dapmpening times in range.
  - Add await time before executing callback to relax whole update system
  - Paginator never marks first page as `recently_synced`
  - Paginator allows for all incoming updates to trigger callback

### Changed

  - `available_actions_for_conversations` and `available_actions_for_messages`
    now depends on the current view.

## [0.20.0] - 2024-10-31

### Added

  - Methods to get and watch all sessions, not just those of a particular account
  - `Draft::create` - Same as `Draft::new()` but actually shows up in Kotlin bindings.


## [0.19.1] - 2024-10-28

### Fixed

  - Draft create/save mime type in API request.

## [0.19.0] - 2024-10-28

### Added

  - Added `Mailbox::all_mail()` constructor.
  - Added `Draft::save()`
  - Added `Draft::set_subject()`
  - Added `Draft::set_body()`
  - Added `Draft::set_to_recipients()`
  - Added `Draft::set_cc_recipients()`
  - Added `Draft::set_bcc_recipients()`

### Changed

  - Refactored the `available_actions_for_messages()` and
    `available_actions_for_conversations()` functions.
  - `all_available_bottom_bar_actions_for_message()` and
    `all_available_bottom_bar_actions_for_conversations()` now contains
    `local_id` for `Labels`.
  - Remove damping on account and session watchers
  - Drafts only create data on the first call to `Draft::save()`

### Fixed

  - Live query updates should be sent only to the table they apply to


## [0.18.0] - 2024-10-28

### Added

  - Added `get_embedded_attachment()`

### Fixed

  - Reduce error log spam.


## [0.17.0] - 2024-10-28

### Added

  - `Draft::attachments()`
  - `Draft::mime_type()`

### Fixed

  - Fixed user settings update in event loop.


## [0.16.0] - 2024-10-24

### Added

  - Added `paginate_conversations_for_label_with_filter()` and
    `paginate_messages_for_label_with_filter()`.
  - Added `paginate_search()`.

### Fixed

  - Accounts get stuck in `NotReady` state (`NotReady` is returned when
    `NeedTfa`/`NeedMbp` should be returned instead)
  - `getPrimaryAccount()` returns null rather than the next-in-line account when
    the primary account is logged out

### Changed

  - `Message::attachments_metadata` now doesn't return embedded attachments.
  - `Conversation::attachments_metadata` now doesn't return embedded
    attachments.


## [0.15.0] - 2024-10-22

### Added

  - `all_available_bottom_bar_actions_for_conversations` function who return
    available actions for conversations in bottom bar.
  - `GeneralActions::ViewMessageInDarkMode` general action.
  - `Draft` type to create/open draft messages

### Changed

  - `message_body` now returns `MailSessionError` on failure.


## [0.14.0] - 2024-10-17

### Added

  - `Label_as` action for conversations
  - `contact_list` method which returns new set of data objects representing
    contact list
  - `all_available_bottom_bar_actions_for_messages` function who return
    available actions for messages in bottom bar.


### Changed

  - All functions which interact on conversations now use their respective
    actions.
  - `mark_conversations_as_read` requires a `Maibox` rather than
    `MailUserSession`.

### Fixed

  - Fixed panic on overflow when `mark_deleted` for messages could overflow in
    some instances.


## [0.13.0] - 2024-10-11

### Added

  - `NotReady` account state


## [0.12.1] - 2024-10-10

### Fixed
  - Ensure only logged in accounts are returned as primary


## [0.12.0] - 2024-10-07

### Changed

  - `mark_messages_read`, `mark_messages_unread` and `mark_messages_deleted` no
    longer require a label.

### Added

  - Add account and session state directly in StoredAccount/StoredSession
  - Blocking forms of `get_account[s]` and `get_session[s]`
  - Blocking forms of `get_account_state` and `get_session_state`
  - Blocking form of `get_primary_account`
  - `LabelAs` action set labels to a group of messages


## [0.11.56] - 2024-10-03

### Fixed

  - `MailUserContext::image_for_sender` now return None for empty images.
  - Bring back soft delete for messages.

### Added

  - `move_messages` which moves many messages from a label into another.
  - Expose actions to star and unstar messages.

### Changed

  - Rename `Ready` variant of `StoredSessionState` to `Authenticated`


## [0.11.54] - 2024-09-27

### Fixed

  - Adjusted watcher damp time from 5s to 200ms.

## [0.11.53] - 2024-09-25

### Fixed

  - Bring back soft delete for conversations
  - HTML messages no longer have extra padding.
  - Plain text messsages get properly rendered.
  - Revert pagination changes without breaking new API.


## [0.11.52] - 2024-09-25

### Fixed

  - Restored paginator `has_next_page()`
  - Login after log out

## [0.11.51] - 2024-09-25

### Changed

  - Paginators now only work with `next_page()`. The of the result set is
    reached when nothing is returned.
  - Paginator construction loads initial page in the background.
  - Disable contact related events


## [0.11.50] - 2024-09-23

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

  - Removed first argument (`mail_settings`) from
    `MailUserSession::image_for_sender`.
  - `MailUserSession::initialize` now fetches data in parallel.


## [0.11.45] - 2024-09-18

### Added

  - Added the following APIs: `delete_messages`, `mark_messages_read`,
    `mark_messages_unread`.
  - Added `reload()` method to the message and conversation paginators

### Fixed

  - Custom folders order are no more random.
  - The CSS for the html mails has been patched

### Changed

  - `paginate_conversations_for_label` and `paginate_message_for_label` now sync
    data from the server.
  - `Mailbox` syncs 50 elements to match pagination behavior.


## [0.11.44] - 2024-09-12

### Changed

  - Errors for login flow are now returned as contextual Enums
  - `mark_conversations_as_unread` now expects a `Mailbox` rather than
    `MailUserSession`
  - Renamed `available_actions_for_conversation` to
    `available_actions_for_conversations`
      - Paramters changed from Id of the conversation to Vec<Id> of
        conversations and include view - Id of the Label
  - Renamed `available_actions_for_message` to `available_actions_for_messages`
      - Paramters changed from Id of the message to Vec<Id> of messages and
        include view - Id of the Label
  - Reimagined `ConversationAvailableAction` to contain each actions section
    representing final view.
      - renamed to `ConversationAvailableActions`
  - Reimagined `MessageAvailableAction` to contain each actions section
    representing final view.
      - renamed to `MessageAvailableActions`
  - Logs now always contain debug info, except for database debug logs.
  - When `MailSessionParams::log_debug` is set to true, database debug logs are
    also included.

### Added

  - `available_label_as_actions_for_messages` &
    `available_label_as_actions_for_conversations` methods exposing label_as
    actions
  - `available_move_to_actions_for_messages` &
    `available_move_to_actions_for_conversations` methods exposing move_to
    actions
  - `GeneralActions` enum representing static actions on message handled by FE.
  - `ReplyActions` enum respresenting reply options.
  - `IsSelected` enum representing selection state for Move & LabelAs actions.
  - `MoveAction` enum representing folder (either system or custom) to which
    item can be moved to.
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

  - `ExclusiveLocation` enum now instead of listing all system exclusive
    locations, wraps them in `System { name: SystemLabel, id: Id }`


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
  - `mark_conversations_as_unread` now expects a `Mailbox` rather than
    `MailUserSession`


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

  - `apply_label_to_messages()` who apply a label to many messages.
  - `remove_label_from_messages()` who remove a label from many messages.

### Fixed

  - Fixed crash in `watchConversation()`


## [0.11.26] - 2024-09-02

### Fixed

  - Fork with `web-account-lite` as version argument.
  - Excessive transactions in event loop.

### Changed

  - `conversation()` now returns the conversation and the messages.
  - `conversation()` may return null if the conversation is not found.
  - `watch_conversation()` now only returns on handle.
  - `watch_conversation()` may return null if the conversation is not found.
  - `conversation()` and `watch_conversation()` now sync the conversation's
    messages at least once.


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

  - Added new function `DecryptedMessage::get_multipart_data` that clients have
    to use to check if the message is multipart and they should show
    attachments.

### Changed

  - Rework `message_id_to_open` from `Option<Id>` to `Id` on
    `WatchedConversation` type
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
  - `MailUserSession::poll_events` method now require callback_interface
    `EventCallback`.
  - Changed the following methods to be sync
      - `MailSession::create`
      - `MailSession::user_context_from_session`
      - `MailSession::stored_sessions`


## [0.11.20] - 2024-08-27

### Changed

 - Renamed `Mailbox::load_attachment_to_buffer` to `Mailbox::get_attachment`.


## [0.11.19] - 2024-08-26

### Added

  - `Sidebar::all_custom_folders` method (return all custom folders in a flat
    way).

### Changed

  - Split `ContextualLabel` in `SidebarCustomFolder`, `SidebarCustomLabel` and
    `SidebarSystemLabel`.

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

  - `Sidebar::custom_folders` to return all custom_folders in a hierarchical
    way.


## [0.11.16] - 2024-08-23

### Fixed

  - Mail functions which accepted `MailSession` have been updated to
    `MailUserSession` instead.


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
      - `LabelDescription` enum contains `System` field with optional
        `SystemLabel` information

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
