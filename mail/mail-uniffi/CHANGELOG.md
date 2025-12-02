# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [mail-uniffi-v0.159.6] - 2025-12-02

### Fixes

- Only update action state after rebase


## [mail-uniffi-v0.159.5] - 2025-12-01

### Fixes

- Created items should also be rebased


## [mail-uniffi-v0.159.4] - 2025-11-28

### Features

- [ET-5494] Run event loop more often


## [mail-uniffi-v0.159.3] - 2025-11-27

### Features

- [ET-5494] Rebase runtime feature flag


## [mail-uniffi-v0.158.4] - 2025-11-26

### Fixes

- [ET-5478] ics: Support email addresses without `mailto:` prefix


## [mail-uniffi-v0.158.3] - 2025-11-25

### Changed

- Tweak conversation mark read

### Features

- [ET-5429] Rebase ham and phishing
- [ET-5429] Rebase Snooze and Unsnooze

### Fixes

- Rollback of conversations should also fetch messages
- Rollback items also sync missing dependencies


## [mail-uniffi-v0.158.2] - 2025-11-24

### Features

- [ET-5428] Rebase delete conversations and messages

### Fixes

- Rollback items should also rebase (if feature enabled)
- Address validation - dont filter out BYOE


## [mail-uniffi-v0.159.0] - 2025-11-21

### Features

- [ET-5103] Enable rebasing of actions


## [mail-uniffi-v0.158.0] - 2025-11-21

### Changed

- s/Message.exclusive_location/Message.location
- [ET-5183] Rebase LabelAs actions
- [ET-5183] Rebase move message and conversation

### Features

- [ET-5390] Sanitize pasted content
- Support address flags
- P1-271: Skip RSVPs for BYOE addresses
- [ET-5426] Message Metadata is now fetched with conversations
- [ET-5408] Provide DraftCreateMode::Mailto
- [ET-5428] Rebase delete in label
- [ET-5427] Rebase mark read/unread actions

### Fixes

- [ET-5183] Conversation Exclusive location
- [ET-5183] unlabel message unread counter update
- [ET-4469] View in light/dark mode
- [ET-4971] URL query is malformed after stripping UTM
- Support proxied-blocked remote content
- [ET-5183] Always execute on server even if noops are detected
- [ET-5427] Mark conversation unread should be noop if already unread


## [mail-uniffi-v0.158.1] - 2025-11-21

### Features

- Validate whether there is a valid sender address


## [mail-uniffi-v0.157.12] - 2025-11-20

### Fixes

- Upsell telemetry - make sure we are using async runtime


## [mail-uniffi-v0.157.10] - 2025-11-19

### Fixes

- [ET-5274] When replying, sync the replied-to message if its body is missing


## [mail-uniffi-v0.157.8] - 2025-11-14

### Changed

- Unify composer's and message's `loadImage()`


## [mail-uniffi-v0.157.7] - 2025-11-13

### Fixes

- [ET-5384] Fix 5xx handling in queue


## [mail-uniffi-v0.157.6] - 2025-11-12

### Fixes

- [ET-5334] Upgrade html2text


## [mail-uniffi-v0.157.5] - 2025-11-12

### Features

- [ET-3505] Remove font and line-height customization from common CSS
- uniffi: Create AttachmentDataError


## [mail-uniffi-v0.157.4] - 2025-11-10

### Fixes

- stash: Drop cyclic dependency between worker threads and their pools (the "database busy" error)


## [mail-uniffi-v0.157.3] - 2025-11-10

### Fixes

- user-context: Fix cancellation of action queue tasks


## [mail-uniffi-v0.157.2] - 2025-11-07

### Features

- Read-only support for blocked domains

### Fixes

- [ET-3605] Resume background tasks for push notification actions
- [ET-3605] Lower default timeout for push notification action to 30s
- [ET-3605] Move logic to mail-common
- [ET-5315] Explicit cancellation on Drop


## [mail-uniffi-v0.157.1] - 2025-11-06

### Features

- [ET-5026] Support bypassing image proxy

### Fixes

- [ET-3605] Do not fetch message from API in case of notification action
- [ET-3605] Breaking Change - add timeLeft to the push notification action
- Search scroller must also respect message sync rules
- [ET-5274] Do not delete draft body if send is not yet complete
- Dot preserve deleted flag on scroller updates


## [mail-uniffi-v0.157.0] - 2025-11-05

### Fixes

- [ET-5278] Force attachment re-encryption on draft creation


## [mail-uniffi-v0.156.4] - 2025-11-04

### Changed

- Remove Scroller's internal dependency on `Mailbox` object for changing labels

### Features

- [ET-5183] Rebase local state changes


## [mail-uniffi-v0.156.3] - 2025-11-04

### Fixes

- [ET-5276] Exclude request timeout from network failures


## [mail-uniffi-v0.156.2] - 2025-11-03

### Fixes

- Scroller removes its cursor if it points to the item which no longer exists withing the scope of the cursor


## [mail-uniffi-v0.156.0] - 2025-10-31

### Features

- [ET-3431] Add 1.5s delay and skip loading bar during pull-to-refresh

### Fixes

- [ET-5216] Topo-sort labels during initialization
- [ET-5222] Fix invalid isProton setting in message contacts
- [ET-5253] Do not crash if key packets are missing from attachment


## [mail-uniffi-v0.155.2] - 2025-10-30

### Fixes

- Bring back WatchUserStream after accidentally removing it during merges


## [mail-uniffi-v0.155.1] - 2025-10-30

### Fixes

- [ET-5230] Fetch missing dependencies for contact emails


## [mail-uniffi-v0.155.0] - 2025-10-29

### Features

- UpsellEligibility
- Introduce payment method API
- get_payment_method in uniffi


## [mail-uniffi-v0.154.8] - 2025-10-28

### Fixes

- Incoming defaults migration & receiving API should not crash


## [mail-uniffi-v0.154.7] - 2025-10-28

### Fixes

- Enable fallback resolver with custom resolver


## [mail-uniffi-v0.154.6] - 2025-10-28

### Changed

- Tag live query callstacks

### Features

- DraftAttchmentListUpdateStream + WathchUserStream

### Fixes

- Scroller test with change include has a race condition on `next_page` request
- Deleted trash should not reappear


## [mail-uniffi-v0.154.5] - 2025-10-27

### Fixes

- Proper PUT incoming defaults
- Incorrect user look up in account event handler
- Make Mailbox label updates more deterministic


## [mail-uniffi-v0.154.4] - 2025-10-24

### Fixes

- [ET-5030] `change_keywords` is awaiting previous task in order to mitigate possible race condition
- [ET-5030] `change_include` is aborting previous task & additional scroller tests


## [mail-uniffi-v0.154.3] - 2025-10-24

### Fixes

- Feature flags are initialized after event service


## [mail-uniffi-v0.154.2] - 2025-10-24

### Changed

- add Role enum

### Features

- Feature flags - onForeground & reuse session

### Fixes

- Blocking works when previous incoming default was Inbox/Spam
- [ET-5030] Reset search scroller state so change keyword is able to load more than two pages
- [ET-5030] `change_include` preserves `unread_filter`


## [mail-uniffi-v0.154.1] - 2025-10-23

### Fixes

- [ET-5139] Handle unplanned conversation id change in draft reply


## [mail-uniffi-v0.154.0] - 2025-10-22

### Changed

- [ET-5030] Remove include from constructors, and use `change_label` commmon functionality for include
- [ET-5030] Remove unread filter from the constructor of the scroller
- [ET-5030] Further simplify construction of the scroller

### Features

- [ET-5030] Add `change_label` method for a scroller
- [ET-5030] `change_label` recalculates alternative labels
- [ET-5030] Add `change_keywords` method for SearchScroller


## [mail-uniffi-v0.153.0] - 2025-10-22

### Features

- Sanitize email addresses in incoming defaults
- [ET-5097] Scroller fetch new feadback


## [mail-uniffi-v0.152.0] - 2025-10-22

### Features

- [ET-5033] Support recipients with display names
- [ET-5097] force_event_poll_and_wait


## [mail-uniffi-v0.151.0] - 2025-10-21

### Changed

- [ET-5086] Apply message sync rules everywhere
- [ET-5086] Only sync out of date state on open
- Do not leak internal scroller state to the interface.

### Features

- Add payments status api request
- Introduce WatchedFeatureFlags
- Resolve system label by id

### Fixes

- Sync Mailbox's label ids with MailScroller
- search: Keep track of original label id
- Call payment status from UserContext
- Allow watching conversations with all messages
- Make sure we dont show banner in all mail folder
- [ET-4920] Expiration time calculation
- Make sure blocking sender creates only one row in DB
- Trash should stay empty when items were deleted
- [ET-5086] Resync missing conversation messages
- [ET-5086] Handle draft creation failure due to sync issues
- Do not prefetch or overwrite deleted items in scroller remote sources
- [ET-5086] Allow the scroller to bring in new conversation updates


## [mail-uniffi-v0.149.22] - 2025-10-17

### Fixes

- [ET-5068] Remigrate signatures on Android


## [mail-uniffi-v0.149.21] - 2025-10-16

### Fixes

- Resync conversation if message count does not match


## [mail-uniffi-v0.142.20] - 2025-10-16

### Features

- Make Cursor Great Again (Again)


## [mail-uniffi-v0.149.19] - 2025-10-15

### Fixes

- Swipe to adjacent conversations is now true by default
- Temporary - prevent getPrevious from showing the same item if the item was removed
- rsvp: Sanitize attendees


## [mail-uniffi-v0.149.18] - 2025-10-14

### Fixes

- uniffi: Improve MailboxCursor's recovery


## [mail-uniffi-v0.149.17] - 2025-10-14

### Changed

- Remove NotSynced error usage from scroller and extend error test with recovery scenario

### Fixes

- `fetch_new` spawns previous page rq in background so it doesn't block scroller updates while offline


## [mail-uniffi-v0.149.16] - 2025-10-13

### Fixes

- stash: Use `ManuallyDrop` inside `Bond::_commit()`
- Move account updates to a separate event subscriber
- Force event poll


## [mail-uniffi-v0.149.15] - 2025-10-13

### Changed

- Subscribing to tables is now async

### Features

- scroller: Add `change_include()` operation
- Invoke MailScroller::fetch_new on pull to refresh
- [ET-5034] Add anchoring to the `fetch_new` underlying request

### Fixes

- Set 'Type=signup' for get_available_domains


## [mail-uniffi-v0.149.14] - 2025-10-09

### Changed

- Track transactions

### Features

- Introduce MailboxCursor

### Fixes

- Hidden message banner is not shown for all trashed messages
- [ET-2816] Fix draft attachment list not updated


## [mail-uniffi-v0.149.13] - 2025-10-09

### Features

- Can't find what you're looking for? Include Spam/Trash.
- New method `is_message_sender_blocked` for displaying `Block` button on the message details screen
- [ET-2816] Attachment Disposition Swap


## [mail-uniffi-v0.149.12] - 2025-10-08

### Changed

- Following methods `conversation`, `watch_conversation` and `messages_for_conversation` requires new parameter `show_trashed: bool`
- Feature-gate some test only `Conversation` methods

### Features

- Add update_next_message_on_move action
- Add swipe_to_adjacent_conversation setting

### Fixes

- [ET-168] Scroller does not store `stale` pages
- Deleted trash should not reappear


## [mail-uniffi-v0.149.11] - 2025-10-06

### Changed

- Delay event poll until all actions have executed

### Features

- mail_settings_sync


## [mail-uniffi-v0.149.10] - 2025-10-03

### Fixes

- [ET-3605] Make notification actions more resilient


## [mail-uniffi-v0.149.9] - 2025-10-03

### Fixes

- [ET-4867] Update move action to respect keep messages setting
- Count action retries and give possibility to specify max_retries value


## [mail-uniffi-v0.149.8] - 2025-10-02

### Fixes

- [SECBTY-1205] Transform HTTP(S) image sources into proton-http(s)
- [SECBTY-1205] Enable proton-http(s) transformers for drafts and decrypted messages


## [mail-uniffi-v0.149.7] - 2025-10-02

### Features

- Report if there are still actions in the queue

### Fixes

- [ET-4384] Only allow one event loop action to be queued
- [ET-4384] Also update messages when opening converastion
- [ET-4384] Revert default action queue to 1 worker
- Move from Spam to Trash should not add `almost_all_mail` label to the message
- Do not remove category labels when moving to trash


## [mail-uniffi-v0.149.6] - 2025-10-01

### Changed

- Log local ids of event updates

### Fixes

- [ET-4884] Support forwarding emails to non-Proton services
- Do not report network issue to sentry
- [ET-4863] Support sending messages to self-owned external addresses with no encryption


## [mail-uniffi-v0.149.4] - 2025-09-30

### Fixes

- [ET-4833] Migrate mobile signature from prev-gen
- [ET-4875] Styles - Use rem instead of em


## [mail-uniffi-v0.149.3] - 2025-09-29

### Fixes

- Bump attachment upload timeout to 2 minutes


## [mail-uniffi-v0.150.0] - 2025-09-26

### Features

- Expose upsell telemetry endpoints
- Make sure we send upsell events only when user allows it
- Observability background task per user account
- Make sure we respect user settings for pre-login events
- Work stealing for pre-login metrics

### Fixes

- [ET-4839] Handle Storage quota exceeded error
- [ET-4266] Proper validation of expiring message support


## [mail-uniffi-v0.149.1] - 2025-09-25

### Fixes

- [ET-4836] Address signatures should always be present


## [mail-uniffi-v0.149.0] - 2025-09-25

### Changed

- Assign as short user id to queue executor spans
- [ET-4749] Cancel recipient validation task after send

### Features

- [ET-2269] Introduce bulk message unread status

### Fixes

- [ET-4800] [Breaking]: MailScrollerError::Dirty was changed to MailScrollerError::NotSynced and repurposed to signal not fully synced states
- [ET-4806] Remove tracing-oslog until crash is fixed
- [ET-4833] Add a migration for mobile signature newlines
- [ET-4763] Report supported if there is no expiration time
- [ET-4803] Prevent rogue event loop action to be executed at fresh start of the app when user takes action
- [ET-4804] Inherit expiration time when replying to expiring message


## [mail-uniffi-v0.148.0] - 2025-09-24

### Changed

- Stash connection acquire timeouts
- Fetch and apply events 1 by 1

### Fixes

- [ET-4402] search: Respect the `MailSettings.almost_all_mail` setting
- Schedule event pool when entering foreground


## [mail-uniffi-v0.144.3] - 2025-09-24

### Changed

- Update account db connection limit

### Fixes

- Use async version of send, rather than sync


## [mail-uniffi-v0.144.2] - 2025-09-22

### Fixes

- Limit background network test max timeout interval to 30 seconds


## [mail-uniffi-v0.144.1] - 2025-09-22

### Fixes

- [ET-4668] Muon timeout failures


## [mail-uniffi-v0.147.0] - 2025-09-19

### Changed

- [Breaking] Issue Report API

### Fixes

- Event Loop Locking
- [ET-4770] Support moving messages/conversations that don't have exclusive locations
- [ET-4791] Fix double page on background initialized locations


## [mail-uniffi-v0.146.0] - 2025-09-18

### Features

- [ET-4721] Issue Reporter Service
- [ET-4721] Hook up sentry reports for critical errors


## [mail-uniffi-v0.145.0] - 2025-09-18

### Changed

- [Breaking] Remove unused draft errors

### Features

- [ET-3273] Expose `groups` in `ContactDetailsEmail` model for displaying group badges

### Fixes

- [ET-4170] Convert newlines in mobile signatures
- [ET-10407] Password Flow refactor
- [ET-4011] Message Too Large


## [mail-uniffi-v0.143.14] - 2025-09-15

### Changed

- Sync previous page is no longer influencing `fetch_more` directly.

### Fixes

- Scroller should return not synced data immediately if any


## [mail-uniffi-v0.143.13] - 2025-09-12

### Fixes

- Search scroller should now expose `get_items` method in uniffi layer
- Migration for Event Poll action was not hooked up


## [mail-uniffi-v0.143.12] - 2025-09-12

### Fixes

- [ET-4720] Report missing send failure errors


## [mail-uniffi-v0.143.11] - 2025-09-12

### Changed

- Scroller auto calls fetch new when entering foreground

### Features

- Scroller.terminate()

### Fixes

- Fetch new should have task awaiting scoped
- Only use active addresses in send preferences


## [mail-uniffi-v0.143.10] - 2025-09-12

### Changed

- [ET-4011] Return Draft Save error in Recipient Errors

### Fixes

- [ET-4426] Disable table decoration and adjust image replace


## [mail-uniffi-v0.143.9] - 2025-09-11

### Changed

- Tweak default db pool sizes

### Features

- improve performance of initial sync
- On Enter/Exit Foreground

### Fixes

- [ET-4697] Malformed declarations do not affect valid CSS declarations


## [mail-uniffi-v0.143.8] - 2025-09-11

### Fixes

- Missing Event Poll action migration


## [mail-uniffi-v0.143.6] - 2025-09-10

### Fixes

- [ET-4637] Request new fido details after resume_login_flow
- [ET-4637] Request new fido details after resume_login_flow


## [mail-uniffi-v0.143.7] - 2025-09-10

### Changed

- Change Event Poll queueing

### Fixes

- [ET-4685] Do not sanitize mailto urls but only in HREF links
- [ET-4685] Fix form sanitization


## [mail-uniffi-v0.143.4] - 2025-09-09

### Fixes

- Scroller should spawn task on next page regardless of the online state
- [ET-4011] Report attachment validation  errors on draft save
- [ET-4426] Always disable image decoration


## [mail-uniffi-v0.143.1] - 2025-09-08

### Fixes

- [ET-4529] Filter out snooze action on msg list even if its defined in the API that way


## [mail-uniffi-v0.143.2] - 2025-09-08

### Features

- Speedup mark as read and unread
- [draft] speed up {apply, remove}_label


## [mail-uniffi-v0.143.0] - 2025-09-05

### Changed

- [ET-4246] Tether Thread Pool
- [ET-4247] Stash Connection Pool Limits

### Features

- Properly sync unsubscribed to newsletter with the backend
- Add custom ScrollerEq derive macro for scroller list comparisons
- [ET-4442] Implement `ReplaceRange` scroller update and eliminate double diff.

### Fixes

- [ET-2062] Missing HTML wrap of undecryptable bodies
- [ET-2062] Missing HTML wrap of undecryptable bodies
- Scroller is allowed to schedule automatic `fetch_more` in certain circumstances
- [ET-3829] Split addresses into TO & CC for reply all action
- Scroller is not mixing oredered and unordered data and never return an empty update for first page in online env


## [mail-uniffi-v0.142.17] - 2025-09-05

### Fixes

- Do not return attachment timeout error from action
- [ET-4521] Re-check attachment limits in action


## [mail-uniffi-v0.142.15] - 2025-09-05

### Fixes

- Skip already deleted items which have expired


## [mail-uniffi-v0.142.14] - 2025-09-04

### Features

- Add `fetch_new` method to try to get newest items

### Fixes

- [ET-4008] Push Notifications Quick Actions fallback


## [mail-uniffi-v0.142.13] - 2025-09-03

### Features

- Properly sync unsubscribed to newsletter with the backend

### Fixes

- Mobile signature will be disabled by default for paying users, and will be always enabled for free users


## [mail-uniffi-v0.142.12] - 2025-09-03

### Changed

- [ET-4125] Open conversation from push notification

### Features

- [ET-4215] Add `conversation_wiht_sync` and `conversation_without_sync`
- Increase default request timeout from 30s to 60s (special case 120s)

### Fixes

- Action queue lost contexts


## [mail-uniffi-v0.142.11] - 2025-09-02

### Fixes

- [ET-4215] Conversation compare


## [mail-uniffi-v0.142.10] - 2025-09-02

### Features

- Use different default signature on ios and android.

### Fixes

- [ET-4125] Always sync conversation messages


## [mail-uniffi-v0.142.9] - 2025-09-01

### Fixes

- Scroller is allowed to schedule automatic `fetch_more` in certain circumstances


## [mail-uniffi-v0.142.8] - 2025-08-29

### Fixes

- Run immediate checks in the background


## [mail-uniffi-v0.142.7] - 2025-08-29

### Fixes

- Prevent looping on false client status by checking the combined status


## [mail-uniffi-v0.142.6] - 2025-08-29

### Changed

- Expose separate os level network observer
- Check network status on resume_work/on_enter_foreground
- Conversation and message body fetch check OS network status
- Integrate OS level status checks in the scroller
- Scroller schedules fetch_more automatically when offline to be executed once online


## [mail-uniffi-v0.142.5] - 2025-08-29

### Changed

- Enable muon error logs

### Features

- ConnectionMonitor as layer on top of muon::Client

### Fixes

- Do not start ping test when OS tells us we are offline
- Prefetch actions report network errors back to the queue
- Use default retry policy for immediate checks


## [mail-uniffi-v0.142.4] - 2025-08-28

### Features

- mail-scroller: Add lifecycle logs

### Fixes

- Network Monitor Layering
- [ET-4378] Don't block in 'to_user_session' which is already async
- Scroller is permitted to spawn first page request everytime it is visiting empty location


## [mail-uniffi-v0.142.3] - 2025-08-28

### Fixes

- Network monitor service immediate timeout


## [mail-uniffi-v0.142.2] - 2025-08-27

### Fixes

- [ET-4381] Support attachments with multiple content-dispositions


## [mail-uniffi-v0.142.1] - 2025-08-27

### Changed

- Enable prefetching back

### Fixes

- Do not spawn network monitor task on pausable future
- [ET-4538] Fix serialization of `AppFeatures`
- Do not ping when os tells us we are offline
- Mobile settings should now be stored correctly in local database


## [mail-uniffi-v0.142.0] - 2025-08-27

### Changed

- Integrate Network Monitor Service

### Features

- `MailSession::update_os_network_status`

### Fixes

- [ET-4011] Correctly report attachment upload failures
- Network Monitor Service - Ping after os status change
- [ET-4011] Action replace should update dependency types
- [ET-4011] Send should cancel if attachments fail to upload
- Endless event loop enqueues


## [mail-uniffi-v0.141.0] - 2025-08-26

### Features

- Network Monitor Service

### Fixes

- Bug in which snooze_time was eqaul across pages.


## [mail-uniffi-v0.140.0] - 2025-08-26

### Changed

- Add Label as a 4th default action for all toolbars

### Features

- [ET-4432] Hide boring attachments™ on the msg/conv list

### Fixes

- Implement UnexpectedUniFFICallbackError for foreign trait errors


## [mail-uniffi-v0.139.0] - 2025-08-25

### Changed

- Resolver trait now must return an error

### Features

- Add uniffi bindings to restore default action lists

### Fixes

- Correct expected input json for PUT /mobilesettings
- remote images correctly get proxied
- Excessive Ping


## [mail-uniffi-v0.138.0] - 2025-08-22

### Changed

- Extend MobileActions implementation with methods allowing to access any toolbar settings available
- (breaking) Change `all_available_bottom_bar_actions_for_xyz` to `all_available_list_actions_for_xyz`
- uniffi: Remove unused `label_type` argument for `watch_labels()`
- Filter out unnecessary mobile actions in uniffi

### Features

- [ET-4017] Run post login validations after signup too
- Unsubscribe to newsletter via HTTP request
- [breaking change] Use load_image instead of load_embedded_attachment and force remote images to use https
- Introduce `generate_csp_nonce()`
- Add AllConversationActions and ConversationActionSheet types & methods analogical to the message features
- Add `AllMessageActions` mechanism based upon `AllListActions`
- Add MessageActionSheet type build in the same way as message toolbars are
- Add API integration for changing API settings
- Add UpdateMobileActions action and all uniffi bindings required to customize toolbar feature to work
- Bye Bye PDFs
- [ET-1307] Introduce Unleash API endpoint to mail-api
- Expose context builder when building context in mail app
- [ET-4397] Expose HTTP 403 Forbidden errors.
- [ET-1307] Introduce Unleash feature flags service
- [ET-4397] Fixed failing test, assert new Forbidden error.
- [ET-4374] expose `remote_id` in UniFFI in `ContactDetailCard`

### Fixes

- [ET-4235] Stanitize and hide css remote/embedded content
- [ET-4235] Best attempt at sanitizing  css variables
- [ET-4235] Only convert http and https links to href
- [ET-4328] Ensure LabelAs and Move work correctly offline
- [ET-4298] Fix mime type inference for replies
- [ET-4260] Improve alternative routing for Human Verification
- Correct resolved host for challenge server
- [ET-4298] Parse X-PM-MIMETYPE
- labels: Watch `MailSettings` as well
- [ET-4297] Fix invalid order_field scroller records
- [ET-4297] Fix missing message snooze notifications
- Ensure fresh auth info is cached
- Validate queue action state
- Fix typo on Fido2 observability events
- Only issue rollback if there are things to rollback
- Less strict matching for TOTP error
- vcard: Support empty `;TYPE=`
- [ET-4439] Support Windows timezones
- Remove ScrollerEq
- Allow scroller to owerwrite local conversation when it has no labels
- [ET-4426] Do not decorate images and links by default
- Make a first page call in the scroller when total is empty and create missing labels in the conversation on scroller sync
- [ET-4444] rsvp: Support party crashers (just reading)
- Scroller will finish refresh update correctly
- [ET-4412] Update text signatures generated from web
- When scroller finds out on refresh that the label has items to display but they are not currently retivable it will queue `FetchMore` task


## [mail-uniffi-v0.136.0] - 2025-08-21

### Features

- [ET-3702] auto-lock: Add more logs

### Fixes

- Snooze/Unsooze should work with partial data
- provide migrations for old label_as and move_to


## [mail-uniffi-v0.133.1] - 2025-08-21

### Features

- Support custom mobile-side resolver


## [mail-uniffi-v0.135.0] - 2025-08-21

### Features

- Scroller now has `get_items` method returning the current state without reading database


## [mail-uniffi-v0.125.8] - 2025-08-20

### Fixes

- ET-4378 Removed a deprecated check that was stalling the MBP workflow.


## [mail-uniffi-v0.125.7] - 2025-08-19

### Fixes

- Correct Conversation label_as available actions


## [mail-uniffi-v0.132.0] - 2025-08-19

### Changed

- [ET-4349] Extend ReportIssue with additional file paths

### Features

- uniffi: Add resolve_system_label_id()


## [mail-uniffi-v0.125.6] - 2025-08-18

### Fixes

- [ET-4260] Improve alternative routing for Human Verification


## [mail-uniffi-v0.125.5] - 2025-08-18

### Changed

- [ET-4272] Disable prefetching

### Fixes

- [ET-4273] Avoid `tokio::spawn()`
- Do not use tx to inspect db migration state


## [mail-uniffi-v0.131.0] - 2025-08-18

### Fixes

- [ET-4176] Remove FIDO details from db
- Fixed documentation for the safety warnings and update fide_details function ref in doc.
- Silence muon error logs
- Applied post merge FMT.


## [mail-uniffi-v0.130.0] - 2025-08-18

### Features

- Expose HV server and resolved hostname


## [mail-uniffi-v0.129.0] - 2025-08-18

### Features

- Enable PmSignature by default

### Fixes

- [ET-4235] Classify contact card urls [breaking change]
- [ET-4235] Santize Logo and Photo urls


## [mail-uniffi-v0.125.4] - 2025-08-15

### Fixes

- Support EventPoll::register() being called multiple times


## [mail-uniffi-v0.125.3] - 2025-08-14

### Fixes

- Use deref to access boxed data


## [mail-uniffi-v0.125.1] - 2025-08-14

### Fixes

- Address Signature not updating
- Missing body update in `html_for_composer`


## [mail-uniffi-v0.128.1] - 2025-08-14

### Fixes

- Make sure Tokio is in scope


## [mail-uniffi-v0.128.0] - 2025-08-14

### Fixes

- Password mode
- Ensure fresh /auth/info when retrying password change


## [mail-uniffi-v0.127.0] - 2025-08-14

### Features

- Implement CRUD for custom settings (aka mobile signatures)
- Consider composer body mime type in sending

### Fixes

- [ET-4273] Avoid `tokio::spawn()`


## [mail-uniffi-v0.126.0] - 2025-08-14

### Features

- Expose ChallengeLoader::post
- Expose ChallengeLoader::put
- Timeout pause/resume during HV challenge

### Fixes

- [ET-4145] Missing conversions for draft attachment upload errors
- [ET-4235] Strip cid links from all uri sources
- [ET-4235] Strip invalid URI sources


## [mail-uniffi-v0.125.0] - 2025-08-13

### Features

- [ET-3780] Expose change password auth errors

### Fixes

- undo move now works when undoing a move out of inbox
- Scroller snooze-time ordering is now taking max value of time and snooze time instead of snooze time alone
- Moving out of snooze folder will remove snoozed label
- [ET-3948] Restore `Draft.set_body` behavior
- [ET-3905] Marking messages read updates to conversation counters


## [mail-uniffi-v0.124.0] - 2025-08-13

### Features

- Allow for optional behavior, when skipping recovery setup during the sign-up.
- Implement share extension v2.0
- Checking username availability: pass the error message from BE to the client app.
- Expose ApiServiceError::UnknownError string

### Fixes

- [ET-4248] Correctly detect valid proton addresses
- [ET-4191] Fix starring and unstarring
- [ET-3948] Draft auto save
- Fixed doc test builds due to removed dependency
- [ET-4018] Skip subscribed user when doing free accounts post-login-check
- [ET-4244] Handle NeedNewPass state
- [ET-2495] Validate total attachment size and count before upload
- label_as + also archive with no labels selected now behave like a normal move to archive


## [mail-uniffi-v0.122.1] - 2025-08-13

### Fixes

- [ET-4244] Handle NeedNewPass state


## [mail-uniffi-v0.123.0] - 2025-08-11

### Features

- Clear action queue in share extension before running
- [ET-4031] Password validation support when changing password.
- Scroller syncs previous page regardless of the length of the inbox
- [ET-3094] Log out account and clear its state if login fails during post login validation
- Show account state in mail-tui account-switcher
- Add comprehensive support for message view with snooze_time ordering

### Fixes

- Scroller's cursor query does not include `order_field` when interacting with cursors
- [ET-4155] Check the correct recipient list for expiration validation
- [ET-4155] Always treat known proton domains as supported
- [ET-4155] Revalidate recipients on draft open
- Message.snoozed_until is read directly from converstation's label instead of `snooze_time` api field.
- [ET-3992] Update the Account data when receiving a user via event loop.
- [ET-4002] Support multipart emails with local-only attachments
- [ET-3605] Poll event loop after answering a notification


## [mail-uniffi-v0.121.0] - 2025-08-08

### Features

- [ET-2602] Add unable to decrypt message body banner

### Fixes

- [ET-2602] Store undecryptable message bodies
- [ET-4176] Use fresh fido2 details in change pass flow


## [mail-uniffi-v0.120.0] - 2025-08-07

### Changed

- Move marking as read for snooze reminder to the common library
- (breaking) Rename `mail_uniffi::Message::snooze_time` into `snoozed_until` and make field optional

### Fixes

- [ET-4142] Handle expiration time too soon error [Breaking Change]
- [ET-4142] Expiration time should be between 15min and 28 days
- rsvp: Support Apple-style invites
- [ET-4124] Check expiration time still valid when sending
- [ET-4142] Report missing send error reasons
- rsvp: Discriminate between network failures and missing events
- [ET-4142] Do not set expiration time on Drafts before send


## [mail-uniffi-v0.119.0] - 2025-08-07

### Changed

- remove the following functions: `remove_label_from_conversations` `apply_label_to_conversations` `remove_label_from_messages` `apply_label_to_messages`

### Features

- [ET-3094] Limit simultaneously logged in free account count
- [ET-4106] Tidied up payment observability events.
- Implement proxying and add load_image fn which proxies images. s/EmbeddedAttachmentInfo/AttachmentData.

### Fixes

- [ET-3926] Undo move and rollback correctly mark as unread and undoes the unlabelling
- [ET-4100] Only fetch message metadata when resolving remote id
- move_conversations and move_messages no longer return VoidActionResult
- [ET-4124] Fetch event in "groups"
- [ET-4125] Do not prefetch attachments when prefetching message body


## [mail-uniffi-v0.118.0] - 2025-08-06

### Features

- Parallel Default Action Groups
- ET-3917 Sign-In: add Observability
- [ET-4083] Expiration Time Options
- [ET-3911] add record_human_verification_view_loading_result function
- [ET-3955] Added payment specific observability events.

### Fixes

- [ET-2416] sort accounts by name


## [mail-uniffi-v0.115.8] - 2025-08-06

### Fixes

- Ordering conversation by snooze time in Inbox
- correctly hide `snooze reminder` on `Conversation::mark_read` and set uniffi conv time to snoozed when required
- shared_status test by using common shared instance of StatusWatcher instead of production one
- Show snoozed banner for all msgs inside snoozed conversation
- Message ordering and banners


## [mail-uniffi-v0.117.0] - 2025-08-05

### Features

- [ET-4084] Draft recipient expiration feature check


## [mail-uniffi-v0.116.0] - 2025-08-05

### Features

- [ET-3911] add record_human_verification_screen_view
- [ET-3911] add record_human_verification_result
- Action Auto dependencies

### Fixes

- [ET-4011] Improve error handling for attachment uploads


## [mail-uniffi-v0.115.7] - 2025-08-05

### Fixes

- rsvp/uniffi: Support attendee-less reminders


## [mail-uniffi-v0.115.5] - 2025-08-04

### Fixes

- [ET-4056] Snooze as an action is only available in Inbox label
- [ET-4054] Include Snooze on conversation action sheet
- rsvp: Use `CalendarEvent.AddressID`


## [mail-uniffi-v0.115.4] - 2025-08-04

### Fixes

- [ET-4052] Snooze is not available as an action in AllMail label
- rsvp: Fetch address keys of the address that has created the calendar
- Disable free account count post login check


## [mail-uniffi-v0.115.6] - 2025-08-04

### Fixes

- rsvp: Support attendee-less reminders


## [mail-uniffi-v0.115.3] - 2025-08-03

### Features

- Support sorting by snooze times
- mail-common: Generate snooze banner
- On unset display_snooze_reminder on mark_read for conversations and messages

### Fixes

- mail: Fix the "is conversation snoozed?" flag (aka `display_snoozed_reminder`)
- snooze: Bubble errors up
- core-common: Fix-Fix the EDM migration


## [mail-uniffi-v0.115.2] - 2025-08-01

### Fixes

- Share extension uses correct client ID to authenticate
- Remove account tables from user db
- Share extension is loading user key secret
- [ET-4103] Always encrypt outside regardless of encryption settings
- rsvp: Support user keys


## [mail-uniffi-v0.115.0] - 2025-07-31

### Features

- [ET-3914] add observability for /domains/available endpoint
- [ET-3914] delete dead code
- [ET-3914] fix wrong test name
- [ET-3914] add observability for unlocking user keys
- [ET-3914] add observability for post login user checks
- [ET-3864] Connect actuall snooze implementation with uniffi layer

### Fixes

- Standardize FIDO2 observability events
- [ET-3503] Allow email address with long local parts


## [mail-uniffi-v0.114.0] - 2025-07-31

### Changed

- [ET-3484] Remember alias address when changing addresses

### Features

- Support share extension
- Introduce AppDetails structure
- Guard against invalid MailSession usage
- [ET-3864] Add unsnooze action
- Breaking change - Rename `*context` -> `*session` in `MailSession`
- rsvp: Support organizer POV

### Fixes

- [ET-1802] Do not retry 2FA in case of receiving auth error.
- User settings migration


## [mail-uniffi-v0.113.0] - 2025-07-31

### Features

- [ET-3971] Expose watcher functionalities for User tables for uniffi clients


## [mail-uniffi-v0.111.0] - 2025-07-30

### Features

- [ET-3943] Add FIDO2 support to the password change flow


## [mail-uniffi-v0.112.0] - 2025-07-30

### Features

- [ET-3094] Post login check for free account count
- [ET-3956] Add uniffi bindings for snooze actions
- [ET-3538] check for user delinquent flag during post login verifications
- [ET-3956] Adjust bindings to discussion
- Add observability API for FIDO screen views needed for clients.
- [ET-3864] Add Snooze action
- [ET-3865] Calculate snooze options
- mail-uniffi: Expose RSVPs

### Fixes

- [ET-3787] Vanishing PGP attachments
- clippy.
- [ET-3630] Add missing get_password method to `Draft`


## [mail-uniffi-v0.110.0] - 2025-07-29

### Features

- [ET-3674] Add change password observability events
- [ET-2067] Replace `primary_at` with `primary_seq`

### Fixes

- [ET-3324] Do not allow replies with invalid addresses
- [ET-3932] Handle invalid address on `Draft::open`
- add format to with_context


## [mail-uniffi-v0.109.0] - 2025-07-28

### Changed

- ContactEmailItem::id -> ContactEmailItem::contact_id

### Features

- Implement undo actions for move_to

### Fixes

- Fully rework how move_to works + Several move_to bugfixes
- `contact_group_by_id` now returns all emails, and emails are always properly sorted in groups in the list.
- Use muon for demo hv webview
- Better HV


## [mail-uniffi-v0.108.0] - 2025-07-25

### Features

- [ET-3867] Add snooze to bottom bar & conversation actions
- [ET-3631] Expiring messages


## [mail-uniffi-v0.105.9] - 2025-07-25

### Fixes

- Marking conversation unread without messages


## [mail-uniffi-v0.105.8] - 2025-07-24

### Fixes

- mail-common: Drop `SaveAsPdf` and `Print` actions
- [ET-3920] Sync missing dependencies in Mail Scrollers


## [mail-uniffi-v0.105.7] - 2025-07-24

### Fixes

- Marking conversations unread
- [ET-2992] Open PGP attachments via `get_attachment`


## [mail-uniffi-v0.107.0] - 2025-07-24

### Features

- [ET-3120] Fido2 support
- [ET-463] Add API call for snooze action
- [ET-3627] Change temporary password during login flow
- Add 'addresses' command to proton-mail-common demo
- [ET-1450] derive Debug for ObservabilityRecorder
- [ET-1450] map ApiError to ApiServiceObservabilityResponse
- [ET-1450] record observability metrics for users/available endpoint
- [ET-1450] record observability metrics for users endpoint

### Fixes

- Fix migration filename
- Present a clear fork API
- Make ObservabilityRecorder.record sync
- Correct error mapping for DuplicateContext variant
- [ET-3706] Prevent duplicate account login
- Fix implementation of first_graphem_uppercase to support emoji.
- Fix snapshot tests.


## [mail-uniffi-v0.105.6] - 2025-07-23

### Fixes

- Incorrect expiration time on new draft messages


## [mail-uniffi-v0.105.4] - 2025-07-23

### Fixes

- Improve scroller offline mode capabilities


## [mail-uniffi-v0.105.2] - 2025-07-22

### Fixes

- [ET-3759] Unable to reply to messages


## [mail-uniffi-v0.105.3] - 2025-07-22

### Fixes

- [ET-3783] Missing attachments


## [mail-uniffi-v0.106.0] - 2025-07-21

### Features

- [ET-3677] Rework label_as and add the undo action to its return type
- [ET-3176] QR Login observability

### Fixes

- Send Client Id from QR code
- [ET-1450] use camelcase for observability enums
- [ET-3760] create_or_get_local to preserve API conversation data over unknown data


## [mail-uniffi-v0.105.1] - 2025-07-16

### Fixes

- Always allow has_mbp etc in password change flow


## [mail-uniffi-v0.105.0] - 2025-07-16

### Features

- [ET-2973] Password Change
- [ET-2974] Support changing mailbox password

### Fixes

- [ET-3550] Do not require scroller refresh while prefetching.


## [mail-uniffi-v0.104.0] - 2025-07-16

### Fixes

- [ET-3758] Use legacy encryption for QR login payload


## [mail-uniffi-v0.99.3] - 2025-07-15

### Fixes

- [ET-3517] Remove height: 100% body style
- [ET-3517] Add 1rem padding


## [mail-uniffi-v0.103.0] - 2025-07-15

### Changed

- [ET-3550] Fetch more returns `ScrollerUpdate::None` when requested by client

### Features

- [ET-3609] Add encrypt-to-outside (EO) crypto logic
- [ET-3630] Send Password Protected Messages
- [ET-2588] Extend contact suggestions to expose contacts in groups

### Fixes

- [ET-3630] Reset password and expiration time on draft open
- [ET-3550] Fix cursor storage, and underlying races, while switching filters in MailScroller
- [ET-3757] Handle muon fork confirmation result
- [ET-3758] QR login should use legacy decryption first then fallback to non-legacy


## [mail-uniffi-v0.99.2] - 2025-07-11

### Features

- Expose fork_with_version

### Fixes

- [ET-3598] Do not allow bypass pin lock without verification on startup


## [mail-uniffi-v0.102.0] - 2025-07-10

### Fixes

- Filename issue: v in front of v008 messing up the db migration


## [mail-uniffi-v0.99.1] - 2025-07-09

### Changed

- Sqlite Performance Tweaks


## [mail-uniffi-v0.101.0] - 2025-07-09

### Changed

- [ET-3550] Add `change_filter` & `force_refresh` methods to the respective scrollers

### Features

- Delete accounts that cannot login
- Add has_a_byoe_address and check during login

### Fixes

- Do not generate keys for users with temporary password


## [mail-uniffi-v0.100.0] - 2025-07-08

### Changed

- `newInboxMailbox` is now sync
- [ET-3550] (Breaking) Reimplement scroller from pull to push model, new API
- new_inbox_mailbox and new_all_mail_mailbox are now sync

### Features

- ET-1970 Update muon reference
- ET-1970 Pass in the info provider to muon. Adapt the device info provider to get the data.
- ET-1970 format code

### Fixes

- Breaking change: Fix Rust handling of keychain errors


## [mail-uniffi-v0.99.0] - 2025-07-03

### Features

- [ET-3592] Add Incorrect2FACode error variant to LoginError

### Fixes

- iOS 0xdead10cc


## [mail-uniffi-v0.98.0] - 2025-07-02

### Features

- [ET-1969] Remove need for clients to pass product name
- Use `fancy_regex` for backend password validators.
- [ET-2969] Add ability to create password validator from user session

### Fixes

- [ET-3497] Make validate fn non-async


## [mail-uniffi-v0.93.21] - 2025-07-02

### Fixes

- update forwarded messages prefix to 'Fw: '
- Reverse CSS for misaligned content, reverse style sanitization
- Cancel action queue workers
- [ET-3321] Improve move_message and move_conversation queries


## [mail-uniffi-v0.93.20] - 2025-06-30

### Fixes

- Sender address for draft conversation
- Remove subscribers when context is dead


## [mail-uniffi-v0.93.18] - 2025-06-27

### Fixes

- [ET-3404] Error message localization
- Correct formatting of UserApiServiceError


## [mail-uniffi-v0.93.19] - 2025-06-27

### Changed

- Suppress Context missing error in event loop subscriber

### Features

- [ET-3025] Add size based log rotator
- [ET-3025] Add `MailSession::export_logs`


## [mail-uniffi-v0.96.0] - 2025-06-26

### Features

- ET-3175 Add EdmOptOut flag
- ET-3175 Add migration code
- ET-3175 Fix failing test
- ET-3175 Connect to the correct db
- contact_group_by_id for android
- [ET-3413] Use crypto crate for QR Login encryption functionalities
- [ET-3497] Provide uniffi bindings for PasswordPolicy

### Fixes

- Do not generate keys for non-private users


## [mail-uniffi-v0.93.17] - 2025-06-26

### Fixes

- Make sure our tracing logger is not crashing the app if fails
- [ET-2322] Update Draft Conversation subject
- [ET-2909] Handle Already Sent via event update
- [ET-3518] Hide Reply Message Actions
- [ET-3310] Handle missing user context on logout


## [mail-uniffi-v0.93.16] - 2025-06-25

### Features

- Add a way to create decrypted message from string for the sake of testing

### Fixes

- Images were stretched
- Deserialization error for fetch label request
- [ET-3422] Improve HTML address signature update
- Incorrect Mail Settings PmSignature bitflag values


## [mail-uniffi-v0.93.15] - 2025-06-25

### Fixes

- [ET-3312] Remove tear down step from sign_out_all
- [ET-3085] Remove address_forwarding_id to fix address key generation
- [ET-2815] Conversation Label Update when messages deleted
- Do not generate keys for non-private users


## [mail-uniffi-v0.93.12] - 2025-06-24

### Changed

- (breaking) remove direct prefetch calls

### Features

- Add prefetching of the very first page of the scroller

### Fixes

- [ET-3312] Don't remove log messages at sign-out
- [ET-3477] MailSetting PmSignature value


## [mail-uniffi-v0.93.14] - 2025-06-24

### Features

- Sanitize <style> from HTML

### Fixes

- [ET-3024] Log ProtonMailError Conversion


## [mail-uniffi-v0.93.13] - 2025-06-24

### Fixes

- [ET-3429] Correctly reply to email+alias


## [mail-uniffi-v0.95.0] - 2025-06-23

### Features

- Submit user behavior data when creating external users.
- [ET-3141] Download password policies
- [ET-3166] Sign-in with QR code
- [ET-1969] Move login challenge payload logic to Rust

### Fixes

- [ET-3404] Error message localization


## [mail-uniffi-v0.93.11] - 2025-06-23

### Fixes

- Make sure the emails are not overflowing the width of the screen
- [ET-3416] (breaking) Add `start_auto_lock_countdown` MailSession method to be used by clients just b4 putting app to background
- [ET-3308] Schedule send now properly update if edited on another session (other edge cases)
- Incorrect DB Migration


## [mail-uniffi-v0.93.10] - 2025-06-23

### Fixes

- [ET-3428] Reply to Simple Login Alias
- sync_conversation_messages are using graceful_status method


## [mail-uniffi-v0.93.9] - 2025-06-20

### Fixes

- [ET-3285] Add `graceful_status` method and use it when trying to fetch messages for conversation or message body
- Do not overwrite conversation if its `known` but has no messages
- [ET-3308] Drafts and schedule send now properly update if edited on another session.


## [mail-uniffi-v0.93.6] - 2025-06-18

### Fixes

- [ET-3378] Do not upload deleted attachments
- [ET-3395] Attachment Key Packets Order
- [ET-3342] Draft Signature Update on Address Change


## [mail-uniffi-v0.93.5] - 2025-06-17

### Fixes

- [ET-3367] For labels with count less than page_size force fetch_more while loading `all_items`
- [ET-2944] Fetch missing dependencies during event loop


## [mail-uniffi-v0.94.0] - 2025-06-17

### Features

- [ET-1969] Construct and send challenge payload
- [ET-1969] Add payload version
- [ET-1969] Use different payload for username and recovery frames
- [ET-1969] Make clippy happy again
- [ET-1969] Encapsulate challenge info in StateData object
- [ET-1969] Use tagged enums
- [ET-1969] Convert nested type to PayloadFrame idiomatically
- [ET-1969] Avoid behaviour type duplication
- [ET-1969] Clarify field purpose
- [ET-1969] Attach user behaviour to submit_internal_username
- [ET-1969] Provide challenge version from Rust
- [ET-3192] Basic password validator.

### Fixes

- Extra space between subject and reply/forward prefix


## [mail-uniffi-v0.93.4] - 2025-06-17

### Fixes

- Do not sync conversation on load without messages
- Disable fixed location prefetcher
- [ET-3243] Mark Conversation Unread
- Conversation Message selection


## [mail-uniffi-v0.93.3] - 2025-06-17

### Fixes

- [ET-3380] Fist call to `should_lock` always returns true


## [mail-uniffi-v0.93.2] - 2025-06-16

### Changed

- [ET-3026] Improve action queue logs

### Fixes

- [ET-3301] Add `biometrics_check_passed` for invokation after biometrics protection was verified by the client
- [ET-3301] Reset the access when auto lock is not invoked


## [mail-uniffi-v0.93.1] - 2025-06-16

### Fixes

- [ET-3307] Disable wal checkpoint on close
- [ET-3351] Out of bounds access in Draft::save


## [mail-uniffi-v0.93.0] - 2025-06-13

### Fixes

- [ET-3260] Change background color to #191927
- Allow unknown message flags to be parsed


## [mail-uniffi-v0.91.0] - 2025-06-13

### Features

- [ET-3083] Post-Login Account Setup

### Fixes

- Change scroller error type from `ContextError` to `MailScrollerError`
- Fix background crash on iOS


## [mail-uniffi-v0.92.0] - 2025-06-13

### Changed

- `force_event_loop_pool` action priority

### Fixes

- [ET-3313] Fix draft stuck in sent folder after send externally
- [ET-3247] Can reply message property
- Label events not applied before contact events
- Add missing email field to contact details and change email.name: String to email.email_type: Vec<VcardPropType>
- [ET-3329] Fix Save Send dependencies
- [ET-3329] Allow draft to save if attachment upload fails


## [mail-uniffi-v0.90.0] - 2025-06-12

### Changed

- Move time validation of Pin and AutoLock out of the database and utilize `Instant`

### Fixes

- [ET-3300] Dissallow manipiulation of time
- [ET-3301] Start counting time for autolock when going to background
- [ET-3212] Mail Scroller prevents double pages by marking itself as a dirty
- [ET-3325] Correctly handle save when already sent
- [ET-3212] Add `MailScrollerDirty` error reason on fetch_more when scroller is dirty


## [mail-uniffi-v0.89.0] - 2025-06-12

### Features

- [ET-3247] Message `is_scheduled` and `can_reply` properties
- [ET-3101] Breaking change - replace `html_head_content_for_composer()` with `html_for_composer()`
- [ET-3101] Introduce list of **trusted** senders
- [ET-3101] lower tracing level when transforming HSL, add support for light-dark

### Fixes

- [ET-3126] Check if online before attempting to cancel scheduled msg
- [ET-3125] Only allow up to 100 scheduled send messages
- [ET-3212] Event Loop should be requested with option to get counters back
- [ET-3178] Fix editor ID in composer
- [ET-3169] Replace path separators from attachment file name
- Save Draft after address change
- [ET-3211] Scheduled send messages should be sorted ascending
- Signatures are always stored in HTML


## [mail-uniffi-v0.87.0] - 2025-06-09

### Fixes

- [ET-3092] Add new non-crashing `sign_out_all` method on `MailSession`
- [ET-3092] Remove old standalone `sign_out_all` function
- [ET-3092] Clear rust logfiles on `sign_out_all`
- [ET-3261] Make sure light mode on Android works as expected


## [mail-uniffi-v0.86.0] - 2025-06-09

### Features

- [ET-3178] List of untrusted_senders
- Action Queue auto dependency keys
- Added DeviceInfoProvider foreign trait.

### Fixes

- [ET-3178] Fix some ugly colors by also taking lightness into the consideration when calculating achromatic colors
- [ET-3178] Support for -webkit-text-fill-color
- [ET-3178] Fix a case where color is not recognized by CSS parser
- Obey remaining attempts on pin validation
- [ET-3093] Sign out all accounts on delete pin when max attempts is reached
- [ET-3096] Clear app protection on last account log out
- [ET-3259] Remove innerHTML on certain tags


## [mail-uniffi-v0.85.0] - 2025-06-06

### Features

- [ET-2568] Attach public key to messages
- [ET-2568] Removing public keys is considered an error
- ET-3080 Add email and phone validation
- ET-3080 Fix build errors
- ET-3080 Fixes
- ET-3080 Run cargo fmt
- ET-3080 Fix clippy issues
- [ET-3178] Sanitize deprecated HTML attributes for dark mode
- [ET-3090] Autolock defaults to 15 minutes and is respected on first lock
- [ET-1410] Sender Address Change

### Fixes

- [ET-2066] Do not use sender when replying to a sent message
- [ET-3021] Update list of proton colors used for avatars
- Convert HTML signature to plain text
- report phishing moves the conversation too
- Several bugfixes to move-to and label-as.
- [ET-3178] Use different selector for IDs that works with spaces
- [ET-2568] Removing public keys is not considered an error
- [ET-3178] Multiple tags do not duplicate style overrides
- [ET-3178] Fix a case where there are multiple classes in one tag
- Remove Attachment should not be cancelled if upload failed


## [mail-uniffi-v0.84.0] - 2025-06-04

### Features

- [ET-3101] Do not remove !important flag from stylesheets
- [ET-3101] Insert extra id to the HTML root
- [ET-3101] Expose `html_head_content_for_composer()` method in drafts
- [ET-3101] Removing !important from style attributes is reversible
- [ET-3101] Revert dark mode before saving a draft


## [mail-uniffi-v0.83.0] - 2025-05-28

### Features

- [ET-2140] Resync event for Mail EventLoop
- move LoginFlow to account crate
- Contact email type now has name and avatar information, and groups now use contact emails instead of contacts

### Fixes

- Revert renaming CoreSession
- ios fixes: conflicting named enums
- DraftSendResultWatcher not triggering after send


## [mail-uniffi-v0.82.0] - 2025-05-27

### Features

- [ET-3101] Move styles to body
- [ET-3101] Move styles to body in draft reply
- Expose delivery time in DraftSendResult

### Fixes

- [ET-2742] Replies to html messages should always be in HTML
- [ET-3021] Add default implementation for avatar information (e.g. draft)


## [mail-uniffi-v0.81.0] - 2025-05-26

### Features

- [ET-3101] Replying to message creates a draft with only a body of the message

### Fixes

- Registering push notification waits for the network in the offline mode
- Correct cached status for conversations created by a draft
- Attachments get downloaded again if they're deleted from disk
- Conversation Create race condition
- [ET-2328] Move Draft to Sent folder on Already Sent Error
- Update watcher create order to avoid races


## [mail-uniffi-v0.80.0] - 2025-05-22

### Features

- [ET-735] Account Sign-Up Flow


## [mail-uniffi-v0.79.0] - 2025-05-19

### Changed

- [ET-2814] Remove DecryptedMessage::body_with_defaults
- [ET-2814] Make ThemeOpts optional

### Features

- Add `Never` option for `AutoLock` App Setting
- [ET-2814] Dark mode in older devices that do not support media query
- [ET-621] Cancel Schedule Send
- [ET-619] Enable Schedule Send Banner


## [mail-uniffi-v0.78.0] - 2025-05-16

### Changed

- Add RawEvent data structure to unbound event_loop::Provider from generic

### Features

- [ET-2814] Support Dark Mode in messages
- [ET-2814] Add default properties for TransformOpts
- [ET-2814] Parse CSS in order to dynamically inject supplement
- [ET-642] MailSettings::should_auto_lock method for keeping track if autolock setting allows for app protection invokation
- [ET-2814] Dark mode handles also inline styles
- [ET-574] Allow to disable alternative routing in AppSettings
- Draft::get_embedded_attachment_sync

### Fixes

- [ET-2814] Use HTML namespace for inserting links
- Lower priority of Prefetch actions


## [mail-uniffi-v0.77.0] - 2025-05-14

### Changed

- tui: Remove unused dependencies

### Features

- [ET-2892] Mailbox::recipient_display_mode
- [ET-619] Add support for schedule send

### Fixes

- Outbox should have message view mode.
- Injecting style should not escape GT and LT signs


## [mail-uniffi-v0.76.1] - 2025-05-08

### Fixes

- Spam banner only shown for messages marked as such by the backend
- [ET-2895] Do not remove inner HTML when sanitizing content


## [mail-uniffi-v0.76.0] - 2025-05-07

### Fixes

- [ET-2786] Only save draft if body and/or subject differ


## [mail-uniffi-v0.75.5] - 2025-05-06

### Fixes

- Properly calculate when messages will get autodeleted.
- The phishing banner was shown for suspicious messages, now the spam banner is shown instead


## [mail-uniffi-v0.75.4] - 2025-05-05

### Fixes

- Run 'cargo generate-lockfile' for proton-mail-uniffi@0.75.4


## [mail-uniffi-v0.75.2] - 2025-04-30

### Fixes

- Ensure last conversation messages is marked as unread


## [mail-uniffi-v0.75.0] - 2025-04-28

### Changed

- [ET-2328] Do not report errors on already sent


## [mail-uniffi-v0.72.13] - 2025-04-25

### Fixes

- [ET-2763] MailScroller will not send data requests when offline
- [ET-2754] Draft now holds weak handle to `MailUserContext`.
- [ET-2754] Decrypted message now hold s weak handle to the `MailUserContext`
- Do not sync mailbox, fix of b8a6329bb0fe62a7cc3acc8676ea6553d3f2eeca


## [mail-uniffi-v0.72.12] - 2025-04-24

### Fixes

- [ET-2765] Cleanup background tasks when session is revoked
- [ET-2765] Make sure we remove contexts from session map
- [ET-2765] Make minor adjustments


## [mail-uniffi-v0.72.11] - 2025-04-23

### Fixes

- [ET-2698] Handle a case where authentication scope is not enough


## [mail-uniffi-v0.72.10] - 2025-04-23

### Fixes

- Remove event debug print
- One transaction per event


## [mail-uniffi-v0.73.0] - 2025-04-17

### Changed

- [ET-2678] Optional filename overwrite for new attachments

### Features

- AttchmenList::add_inline
- [ET-2719] Delete all folder banner, uniffi + tui
- Support /payments/resources/icons
- GoogleRecurring PaymentReceipt (post_payments_tokens).

### Fixes

- Only allow one session per user
- Leaking tracing spans in async code
- Return cached data from mail scroller immediately
- Don't hang if transaction never ends
- don't send rollback again


## [mail-uniffi-v0.72.9] - 2025-04-17

### Changed

- Synchronize db writes

### Fixes

- Restore stash debug filter for iOS logs
- Network requests in send preferences tx


## [mail-uniffi-v0.72.7] - 2025-04-16

### Fixes

- Return cached data from mail scroller immediately


## [mail-uniffi-v0.72.6] - 2025-04-14

### Fixes

- Restore manual event loop polling with `force_event_loop_poll`


## [mail-uniffi-v0.72.5] - 2025-04-14

### Changed

- MailUserContext polls events in the background

### Features

- Ensure that html content is encoded as base64 in mime

### Fixes

- [ET-2326] Sanatize the conent-id in the mime-buidler
- Disable async logger for iOS
- Wake TaskService awaiters on resume


## [mail-uniffi-v0.72.4] - 2025-04-14

### Fixes

- Leaking tracing spans in async code


## [mail-uniffi-v0.72.3] - 2025-04-11

### Fixes

- Ignore duplicate context errors in background tasks


## [mail-uniffi-v0.72.1] - 2025-04-10

### Fixes

- Correct background task execution


## [mail-uniffi-v0.72.0] - 2025-04-10

### Changed

- action-queue: Simplify the online-check

### Features

- [ET-2698] Registered device background task
- [ET-2698] Registering devices tests
- [ET-2698] Make the register_device_task a method of the MailSession
- [ET-2698] More logs
- [ET-2698] Make register_device_task synchronous
- [ET-2698] Make sure that registration task can handle network errors
- [ET-559] signup network requests

### Fixes

- core: Fix status watcher's shared state
- [ET-2601] Improve initialization by waiting when failure of dependency happen


## [mail-uniffi-v0.70.10] - 2025-04-09

### Fixes

- [ET-2699] Blocked banner now properly gets updated on the event loop and block_address now takes a String instead of an id


## [mail-uniffi-v0.70.9] - 2025-04-09

### Changed

- Change pin type from Vec<u8> to Vec<u32>


## [mail-uniffi-v0.70.8] - 2025-04-07

### Fixes

- Allow to specify if abort for Bg execution was called from foreground


## [mail-uniffi-v0.70.7] - 2025-04-07

### Fixes

- Add Action's PAUSABLE associated const which force Executors to finish apply_remote b4 pausing


## [mail-uniffi-v0.70.6] - 2025-04-07

### Fixes

- Make `Draft::save` & `Draft::send` action's `apply_remote` non pausable futures


## [mail-uniffi-v0.71.0] - 2025-04-07

### Features

- [ET-2592] Terminating action queue auto executor
- Add banners to the tui client
- [ET-2592] Background execution terminates when queue is empty
- Support local API server


## [mail-uniffi-v0.70.5] - 2025-04-07

### Fixes

- [ET-2551] Tweak request values of status observer


## [mail-uniffi-v0.70.3] - 2025-04-04

### Fixes

- Escape rendering of forwarded plain text messages


## [mail-uniffi-v0.70.2] - 2025-04-04

### Fixes

- [ET-2581] Don't show embedded and remote banners when there is no remote/embedded content.


## [mail-uniffi-v0.70.1] - 2025-04-04

### Fixes

- Do not delete draft attachments
- [ET-2671] Persist recipient removal


## [mail-uniffi-v0.70.0] - 2025-04-03

### Changed

- [ET-2592] Update background execution API

### Features

- [ET-2552] Use combine contacts AppSetting for `contact_suggestions`
- Mark as phising implementation
- [ET-2640] Add `MailSession::remaining_pin_attempts` method

### Fixes

- [ET-2613] Fill `Username` field in bug report request
- [ET-2614] Correctly format bug report description to include all provided fields
- [ET-2592] Replace r2d2 with basic replacement
- Missing pinned and blocked system label migration


## [mail-uniffi-v0.69.0] - 2025-04-02

### Features

- [ET-2552] Clear application data on 10th incorrect PIN validation attempt
- [ET-2552] Add functionality to set/unset biometrics
- [ET-2552] Cancel all tasks spawned in UserCtx
- [ET-2552] Archive database files

### Fixes

- task-service: Improve pausable futures
- task-service: Add another test
- [ET-2551] Make status observer more resilient in poor connection environment
- task-service: Refactoring, improve docs
- task-service: Address comments
- [ET-2358] Correct reported error after draft deleted
## [0.68.2] - 2025-04-02

### Fixed

  - Repeated action registration crash

## [0.68.1] - 2025-04-01

### Fixed

  - Fixedbitset insert crash.

## [0.68.0] - 2025-04-01

### Added

 - `MailSession::pause_work_and_wait` - Should be called when the application enters the background
    to ensure db locks are released.
 - `MailSession::pause()` - Should be called when the application enters the background
    to ensure db locks are released, but this version does not wait until all background work is
    paused.
 - `MailSession::resume_work()` - Should be called when the application enters the foreground.

### Fixed

  - Double action registration assert.

## [0.67.0] - 2025-04-01

### Added

  - `block_address`, `unblock_address` functions are available.

### Fixed

  - Double initialization crash.
  - Observability names.

## [0.66.0] - 2025-03-28

### Added

  - [ET-2552] `AppSettings`, `AppSettingsDiff`, `AppAppearance`, `AppProtection`, `AutoLock` datatypes to interact with `AppSettings`
  - [ET-2552] `PinAuthError`, `PinAuthError` error types to propagate User friendly errors when interacting with PIN interface.
  - [ET-2552] `MailSession::app_protection` method for checking current protection configuration.
  - [ET-2552] `MailSession::set_pin_code` method for configuring new PIN code.
  - [ET-2552] `MailSession::verify_pin_code` method for verifing existing PIN code.
  - [ET-2552] `MailSession::delete_pin_code` method for deleting existing PIN code.
  - [ET-2552] `MailSession::get_app_settings` method for fetching current `AppSettings`.
  - [ET-2552] `MailSession::change_app_settings` method for modifing current `AppSettings`.

### Removed

- [ET-2558] MailUserContext initialization is no longer explicit. Function has been removed.

### Changed

- [ET-2558] MailUserContext is now initialized whenever client acquires a new one.
  - It may fail if the device is in the offline mode.
  - It is mobile dev responsibility to handle that.

### Fixed

  - The spam banner will stop appearing after it's been marked as legitimate.

### Added

- [ET-2601] `initialized_user_context_from_session` - returns MailUserContext but only if it existed and was initialized before.

## [0.65.3] - 2025-03-27

### Fixed

  -  [ET-548] Issue report now add client version field to the API request

## [0.65.2] - 2025-03-26

### Fixed

  -  [ET-548] Issue report now add email field to the API request

## [0.65.1] - 2025-03-26

### Fixed

  - Crash in observability code.

## [0.65.0] - 2025-03-26

### Added

  - [ET-2438] PGP/Mime attachment forward + Reply support.
  - Expose user settings via `MailUserSession::user_settings` method
  - [ET-736] Human Verification
  - `SystemLabel` have their values again.

### Fixed

  - Draft body not being saved
  - Made `icon_name` optional in `Entitlement::Progress`

## [0.64.0] - 2025-03-25

### Added

  - [ET-736] Human Verification

## [0.63.0] - 2025-03-24

### Fixed

  - Call `disconnect()` when dropping `DraftAttachmentWatcher` and `DraftSendResultWatcher`.

### Added

  - [ET-548]: Report an issue feature is fully implemeneted
  - SystemLabel has two new locations: `Blocked`, `Pinned`.


## [0.62.1] - 2025-03-24

### Fixed

 - Log spam from deferred draft attachment cleaner.


## [0.62.0] - 2025-03-24

### Added

 - [ET-2492]: Auto clean attachments uploaded to draft staging area.

### Changed

  - `mark_messaages_ham` now takes `mailbox` for consistency and a single `id` and is called `mark_message_ham`.
  - Improved db tracking lock behavior.

### Fixed

  - Call `disconnect()` when dropping `DraftAttachmentWatcher` and `DraftSendResultWatcher`.
  - The watcher also reloads when new attachments exist.
  - `DraftAttachmentObserver` also reacts to removal.

### Removed

  - `DecryptedBody::get_all_attachments` was deleted as it is no longer necessary.


## [0.61.0] - 2025-03-20

### Added

  - `AttachmentList::remove`: Remove attachments from a draft.

### Fixed

  - Fix save issue when attachment are present in a reply/forward draft.


## [0.60.0] - 2025-03-20

### Added

  - `rust_sdk_version`: Get the rust sdk version string.
  - `rust_sdk_version_[major|minor|patch]`: Get individual version components of for the rust
     sdk version.
  - `is_valid_email_address`: Validate whether an email address is correctly formatted.
  - `report_an_issue` method for performing API report
  - `IssueReport` struct for creating a report

### Fixed

  - MailScroller & Prefetcher now stores only non existent data.
  - DratAttachmentState now has the error reason.
  - Subscription ID is optional



## [0.59.0] - 2025-03-17

### Fixed

 - [ET-2494] Preserve failed attachment uploads in drafts.


## [0.58.1] - 2025-03-14

### Fixed

  - Bumped `html2text` to 0.14.2 to fix compile errors


## [0.58.0] - 2025-03-14

### Added

  - [ET-2450] `LoginFlow` has a new method `migrate` for migrating from legacy application.


## [0.57.0] - 2025-03-13

### Changed

  - Rename `auth_id` to `session_id` in `EncryptedPushNotification` type.
  - Move the banners from the conversation to the decrypted body.

### Fixed

  - [ET-2001] Account avatar not showing initials when display_name is an empty string.
  - Fixed the pgp attachment crash.


## [0.56.0] - 2025-03-11

### Added

  - [ET-2374] Only for Android: Added field `action` to the email push notification.
    This field represents whether it is a new message or a silent notification.
  - [ET-2394] Expose message banners (new field in `WatchedConversation`)

### Changed

  - [ET-2367] Key chain takes `OSKeyChainEntryKind` parameter. Key chain implementation must support
     more than one entry at the time.
     - get -> load method rename
  - [ET-2368] Saving registered device is now generating new device token key pair if necessary.
  - [ET-2368] Registering device is now using device key instead of user key.


## [0.55.5] - 2025-03-17

### Fixed

  - Only persist encrypted password when not using separate mailbox password


## [0.55.4] - 2025-03-17

### Fixed

 - [ET-2131] Fix key packets missing error when sending messages.


## [0.55.3] - 2025-03-13

### Fixed

  - `watch_conversations` now correctly observes changes in the message's labels


## [0.55.2] - 2025-03-12

### Fixed

  - [ET-2434] Do not panic on `delete_conversations` action when label id does not have remote id.


## [0.55.1] - 2025-03-11

### Fixed

  - [ET-2288] Multiple Stash improvements for database stability and mitigating random crashes


## [0.55.0] - 2025-03-07

### Added

  - [ET-2141] `get_unsent_messages_ids_in_queue` method on `MailSession` to get MessageIds of unsent messages for given user_id

### Fixed

  - [ET-2317] Restore forward/reply with attachments.

## [0.54.4] - 2025-03-06

### Fixed

  - Cyclic dependencies when saving drafts

## [0.54.3] - 2025-03-05

### Fixed

  - Reduce log spam from event loop if there is not network.
  - Do not trigger callback on abort of background execution
  - All executor are on MailUserContext to avoid a state where executor is in never ending error loop

## [0.54.2] - 2025-03-04

  - [ET-2241] Rely on auto executors in `start_baground_execution`


## [0.54.1] - 2025-03-04

### Fixed

  - No longer crashes if we try to decrypt attachments without key packets.
  - [ET-2141] Queue executors now wait for network to be restored before retrying.
  - More error logging when pinned key extraction fails.

## [0.54.0] - 2025-03-04

### Added

  [ET-1470] `AttachmentList::retry` to upload attachments

### Fixed

  [ET-2295] - Crash on due missing error handling logic.
  [ET-1470] - Do not override attachment key packets and signatures if they exist.


## [0.53.0] - 2025-03-04

### Added

  - [ET-2293] `RustInit::init_tls` to initialize the TLS subsystem on android


## [0.52.3] - 2025-03-04

### Fix

  - [ET-1470] Make sure draft save saves the correct state.


## [0.52.2] - 2025-03-04

### Added

  - [ET-2204] `create_mail_ios_extension_session` - more resource constrained sibling of `create_mail_session`, designed
  for notification extension
    - It spawns lower number of async runtime workers,
    - It also limits number of DB connections from 100 to 4


## [0.52.1] - 2025-03-04

### Fix

  - Stash's connection pool should now be less susceptible for random errors due to 262 SQLITE_LOCKED_SHAREDCACHE


## [0.52.0] - 2025-03-03

### Added

  - [ET-1407] `AttachmentList` for `Draft` type.
  - [ET-1407] Attachments of disposition attachment can now be added to a draft and sent.

### Changed

  - [ET-1407] `Draft::attachments` is removed in favor of `Draft::attachment_list`.


## [0.51.0] - 2025-03-03

### Added

  - [ET-2204] `resolve_message_id` translating remote id into local id with necessary API lookup.
    - `RemoteId` has been added but should be used only as a last resort. If possible local `Id` is preferable.
  - [ET-2241] `start_background_execution` method on `MailSession`, to finish any peding tasks before app is terminated.
  - [ET-2241] `all_messages_were_sent` method on `MailSession`, to verify if are messages were send before putting app to sleep.

### Removed

  - `Context.is_network_connected` and associated traits and methods
  - `MailSession::new` does not accept optional network_callback anymore

### Fixed

  - [ET-2260] Undoing drafts is no longer causing "Opened a non-draft message as a draft".
    - Additionally prefetching and mail scroller is no longer overriding draft message metadata.

### Changed

  - [ET-2204] Decrypting push notification does not translate remote id into local id. Use `resolve_message_id` instead.
    - This hopefully resolves Out of Memory issue for the push notification extension.


## [0.50.0] - 2025-02-27

### Added

  - Dummy interfaces for human verification


## [0.49.0] - 2025-02-27

### Added

  - [ET-1955] `MailUserSession` has new method `execute_when_online` which accepts standard callback.


### Changed

  - [ET-2204] Decrypted push notifications contain now valid and usable payloads for emails and opening urls.


## [0.48.1] - 2025-02-25

### Changed

  - [ET-2142] Draft actions now run in their own separate queue.

### Fixed

  - Ensure `watch_contact_list` callback is constructed inside `uniffi_async` wrapper

## [0.48.0] - 2025-02-24

### Added

  - [ET-405]  Two `DeviceEnvironment` cases for ET: sandbox and production
  - [ET-2204] Added `EncryptedPushNotification` and method for decrypting it. The body of the notification is yet to define, but it is a proof of correct message decryption.

### Changed

  - [ET-2182] Conversation and message scrollers now fetch new message on first use.

### Fixed

  - [ET-2205] Mail settings are now stored as always one row. This prevents a bug where mail settings were properly updated and retrieved only once, requiring fresh reinstall
  whenever user changed setting.
  - Random crashes.


### Removed

  - [ET-2142] `excute_pending_action` and `execute_pending_actions` have been removed.


## [0.47.2] - 2025-02-20

### Fixed

  - Use `std::thread::spawn` instead of `tokio::task::spawn_blocking` to spawn the stash tether worker,
    fixing a crash that occurred when the worker was spawned before the tokio runtime itself.

## [0.47.1] - 2025-02-20

### Changed

  - [ET-2095] Added support with never visited locations. On order break (going back Online) the scroller will trigger callback.

## [0.47.0] - 2025-02-20

### Changed

  - [ET-2095] Reverted all Set changes and bring back old scroller behaviour on offline data

## [0.46.0] - 2025-02-19

### Added

  - Put back `VoidFooResult` type instead of method-specific types

### Fixed

  - Replace should return all visible items not fetched ones

## [0.45.0] - 2025-02-19

### Changed

  - Password is now stored temporarily in the database during 2FA stage of the login flow;
    it is encrypted in the same way as the auth tokens and is removed when 2FA is successful.

## [0.44.0] - 2025-02-19

### Added

  - [ET-2095] `ConversationScrollerSet` and `MessageScrollerSet` enum wrappers on respective Vec types to represent append or replace actions on `fetch_more` method call.

### Changed

  - [ET-2095] `ConversationScroller::fetch_more()` now returns `ConversationScrollerSet` instead of the `Vec<Converation>`
  - [ET-2095] `MessageScroller::fetch_more()` now returns `MessageScrollerSet` instead of the `Vec<Message>`
  - Change the strong reference to `MailUserContext` inside `MailUserSession`, `Mailbox` and `Sidebar` into a weak reference
  - Return errors from any method when the weak reference to `MailUserContext` fails to be upgraded to a strong reference

### Removed

  - Replace `VoidFooResult` type with method-specific types

### Fixed

  - [ET-404] A typo in function name "devide" -> "device"

## [0.43.0] - 2025-02-18

### Added

  - [ET-404] Functions for registering and retrieving cached device tokens, used for push notifications

## [0.42.0] - 2025-02-14

### Added

  - [ET-2165] Add standalone `draft_discard` function.

### Fixed

  - [ET-2903] Sent message no longer appears in drafts.
  - [ET-1972] Do not update Draft with pending changes on `Draft::open`.
  - [ET-1976] Swipe gesture returns an error instead of crashing if the label is missing local id

## [0.41.1] - 2025-02-12

  - App no longer crashes when requesting Draft id.

## [0.41.0] - 2025-02-11

### Changed

  - [ET-1976] If user is already in Trash/Spam/Archive, then instead moving to Trash/Spam/Archive, return NoAction
  - Breaking change: `assigned_swipe_actions` takes an additional paramerer `current_folder` which is local label id.

### Fixed

  - [ET-2099] Emojis can be used in externally sign-only messages.
  - [ET-2092] External messages sent to contacts with the sign flag are now signed.
  - [ET-1825] Fix missing html escape in signature.

## [0.40.8] - 2025-02-06

### Fixed

  - Correct state transition when entering wrong mailbox password

### Added

## [0.40.7] - 2025-02-06

### Added

  - [ET-1976] `assigned_swipe_actions` returns what actions are assigned to swipe gestures including
    necessary context information needed for executing them (for example what is Trash/Spam/Archive local label id)

### Fixed

  - Support retrying initial login flow stage

## [0.40.6] - 2025-02-05

### Fixed

  - Added snooze mobile action

## [0.40.5] - 2025-02-05

### Fixed

  - Fix failed conversation updates during draft save.
  - [ET-2039] Scroller now reports an error when there is no more cached data and the api responds with an error.

## [0.40.4] - 2025-02-04

### Fixed

  - [ET-2028] Scroller now early exit for not seen location with error network.
  - Fix false error during `delete_account`.
  - Fix jitter during draft update.
  - Fix missing uniffi async wrapper in `new_event_loop_observer`.


## [0.40.3] - 2025-01-31

### Fixed

  - Fix contact order sorting.
  - Fix ensure all background rust tasks are killed on logout.

## [0.40.2] - 2025-01-31

### Fixed

  -[ET-2032] function `conversation` now early returns when app is offline.
  -[ET-2032] function `get_message_body` now early returns when app is offline.
  -[ET-2007] unread filter now works correctly for message scroller
  -[ET-2022] paginator does not initialize until the first page is fetched and ready to be returned
  -[ET-2014] & [ET-2012] create message and conversation counters when label event is received.

## [0.40.1] - 2025-01-31

### Fixed

  - [ET-1953] Fix prefetch not working after closing the app

### Changed

  - [ET-1971] `contact_suggestions` is loading contacts, sorting and deduplicating them
    - It no longer takes `query` parameter, in order to fetch the data only once and not with every keystroke.
      Therefore, it should be called only once when composer is opened and kept in the memory
    - It no longer returns an array of suggestions. Instead, it returns the object that has two methods
      - `.all()` - to get all suggestions
      - `.filtered(query)` - to get filtered suggestions

## [0.40.0] - 2025-01-31

### Added

  - [ET-1999] `Message::is_draft` property.
  - `EventError` now reports refresh error.
  - [ET-1953] Add a `prefetch` method getting key locations most recent 10 items loaded in a background

### Fixed

  - Newline delimiter in HTML draft replies/forward.
  - [ET-1999] Apply `AllDraft` label and `AllMail` to drafts.

## [0.39.0] - 2025-01-29

### Added

  - [ET-1894] `Draft::get_embedded_attachment` to load inline attachments via cid.

### Fixed

  - [ET-1332] Some messages should display better
  - [ET-1913] Support retrying of failed login flow stages
  - [ET-1923] Remove mail settings signature.
  - [ET-1978] New drafts are not marked as being replies.
  - [ET-1987] Fix email address validation for `RecipientList`.
  - Fix sender address repeated in To and CC on draft reply.

### Changed

  - The log file will be append only.

## [0.38.0] - 2025-01-28

### Added

  - [ET-1954] New method on `MailUserSession` `connection_status` and `ConnectionStatus` enum.
  - [ET-1971] `contact_suggestions` function for the composer recipients autocompletion
    - It has a dummy implementation that always return empty list for now.
  - [ET-1956] `EventLoopErrorObserver` and `MailUserSession::obeserve_event_loop_errors`

### Changed

  - [ET-1956] `MailUserSession::poll_events` queues a poll event action. To get the real event loop
    error one must pass an `EventLoopErrorObserver` to
    `MailUserSession::obeserve_event_loop_errors`. Be sure to keep the returned handle alive.

## [0.37.3] - 2025-01-27

### Fixed

  - [ET-1863] Mailbox counter watchers are updated whenever user marks conversation/message as read/unread
  - [ET-1863] System labels (like Outbox) watchers are updated whenever labels change

### Added

- [ET-1944] `SwipeAction::{NoAction, LabelAs, MoveTo}` are now supported.


## [0.37.2] - 2025-01-27

### Fixed

  - [ET-1932] Fix sending of forward/reply to messges with attachments.
    - Does not yet handle PGP/MIME embedded content
  - [ET-1685] Scroller now is able to switch between filters without being stuck on infinite loading.
  - Conversation and Message attachments now only contain attachments with disposition attachment.
   - Use `numAttachments` to get the total attachment count for each type.

## [0.37.1] - 2025-01-24

### Fixed

  - [ET-1685] Scroller now is able to switch between filters without being stuck on infinite loading.

## [0.37.0] - 2025-01-24

### Changed

  - [ET-1896] Draft save actions are now deduped in the queue.
  - [ET-1685] `scroll_search` was renamed to `scroller_search`

## [0.36.1] - 2025-01-24

### Fixed

  - [ET-1926]: Remote images and embedded images are always enabled, disregarding the setting.

### Added

  - [ET-1633] Add async live query callback, use it for new `watch_accounts_async` and `watch_sessions_async` methods

## [0.36.0] - 2025-01-23

### Changed

  - [ET-1633] Change `core_accounts.primary_at` from `u64` to `f64`

### Fixed
 - Fixed a race condition in the initialization regarding label counters.

## [0.35.0] - 2025-01-23

### Added

  - [ET-1864] `account_details` function in `MailUserSession` and `details` function in `StoredAccount` (it contains account name, email and avatar information that needs to be displayed to the user).
  - [ET-1794] `DecryptedMessage::get_attachments` which merges the API attachments and PGP attachments into one for easier client consumption.
  - [ET-1685] `scroll_search` & `SearchScroller` to make server searches
  - [ET-1385] Resolve contact group total for message recipients.

### Changed

  - [ET-1747] `DraftError` has been split into 4 different sub errors:
    - `DraftOpenError` - For creating and opening drafts.
    - `DraftSaveSendError` - For sending and saving.
    - `DraftUndoError` - For undo send.
    - `DraftDiscardError` - For discard.
  - `DraftSendStatus` now contains the number of seconds left for the message to be undo sent.

### Removed

  - [ET-1864] `avatar_information`, `display_name`, `name_or_addr`, `primary_addr`, `username` functions from `StoredAccount` (replaced with `details` function).
  - `MessageAttachments`, `MessageAttachmentsHeaders` and `MessageAttachmentsInfo` have been removed as they are not needed or used.

## [0.34.0] - 2025-01-22

### Added

  - [ET-1417] `Draft::discard` - Discards a draft from the composer.
  - More logging for the html transformations.
  - [ET-679] `draft_undo_send` - Cancel sending of a message.

### Fixed

  - Drafts are moved to outbox before being sent.
  - [ET-503] Drafts can not be updated after being sent.

### Changed

  - `DraftSendStatus` - now includes whether it can be cancelled or not.

## [0.33.0] - 2025-01-17

### Added

  - `DraftSendResultWatcher` - Observe new send results as they are created.
  - `draft_send_result_unseen` - check all unseen send results.
  - `draft_send_result_mark_read`
  - `draft_send_result_delete`
  - `Draft::send_result` - Loads associated send result with `open_draft` if any is available.
  - [ET-1192] `ContactEmailItem` has two new fields: `is_proton` & `last_used_time`

### Changed

  - `open_draft` now returns `OpenDraft` type which includes whether the body is synced or cached.
  - [ET-1192] `GroupedContacts.item` was renamed to `items`
  - [ET-1192] `ContactGroupItem.email` was renamed to `contacts`
    and now carries `Vec<ContactItem>` instead of `Vec<ContactEmailItem>`

### Fixed

  - [ET-1869] Unable to open drafts in certain conditions


## [0.32.1] - 2025-01-15

### Added
  - function `scroll_conversations_for_label` which utilizes new paginator `ConversationScroller` for conversation in given label. This paginator is based directly on API data which makes it more resilient option than current pagination solution
  - function `scroll_messages_for_label` which utilizes new paginator `MessageScroller` for conversation in given label. This paginator is based directly on API data which makes it more resilient option than current pagination solution

## [0.32.0] - 2025-01-09

### Changed

  - `DecryptedMessage::body` is now infallible.
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
