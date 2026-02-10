# MailScroller Architecture

## Overview

MailScroller is a reactive pagination system for mail items (conversations and messages) that automatically updates when the underlying database changes. It provides efficient scrolling through large datasets with automatic synchronization between local cache and remote servers.

## High-Level Design

The system follows a **façade pattern** with an asynchronous worker architecture:

```
┌─────────────────┐
│   MailScroller  │  ← Public API (façade)
│    (façade)     │
└────────┬────────┘
         │ commands
         ↓
┌─────────────────┐     updates     ┌──────────────────┐
│ ScrollerWorker  │ ───────────────→│ MailScrollerHandle│ ← Client receives updates
│   (internal)    │                 └──────────────────┘
└────────┬────────┘
         │ delegates
         ↓
┌─────────────────┐
│MailScrollerSource│  ← Data source interface
│   (interface)   │
└─────────────────┘
```

## Core Components

### 1. MailScroller (Façade)

**Purpose:** User-facing API that provides simple methods to control scrolling behavior.

**Key Responsibilities:**
- Accept user commands (fetch more, refresh, change filter, etc.)
- Route commands to internal worker via channels
- Provide async query methods (has_more, total, seen, synced)
- Manage lifecycle (subscribe to app events, cleanup on drop)

**Public API:**
```rust
// Pagination
fetch_more()        // Load next page
fetch_new()         // Sync new items from server
has_more()          // Check if more items available

// State management
refresh()           // Re-read from database
force_refresh()     // Force complete reload
get_items()         // Get current snapshot

// Filtering
change_filter()     // Toggle read/unread filter
change_label()      // Switch to different label
change_include()    // Include/exclude spam & trash
change_keywords()   // Search functionality
clear()             // Clear cursor state

// Metrics
total()             // Total items in label
seen()              // Items currently visible
synced()            // Items synced from server
```

**Lifecycle Management:**
- Subscribes to `OnEnterForegroundEvent` → triggers `fetch_new()`
- Subscribes to `OnForceEventPollEvent` → triggers `fetch_new()`
- Aborts background tasks on drop

### 2. MailScrollerHandle

**Purpose:** Client-side handle to receive updates from the scroller.

**Structure:**
```rust
pub struct MailScrollerHandle<T> {
    pub updates: flume::Receiver<ScrollerUpdate<T>>,
    pub handle: DropRemoveTableObserverHandle,
}
```

**Update Types:**

#### ScrollerUpdate<T>
- **Status**: Loading indicators (FetchNewStart/FetchNewEnd)
- **List**: Data changes
- **Error**: Operation failures

#### ScrollerListUpdate<T> Variants
- **None**: No changes detected
- **Append**: New items added at end
- **ReplaceFrom**: Replace from index to end
- **ReplaceBefore**: Replace from start to index
- **ReplaceRange**: Replace range [from..to]

**Update Flow:**
1. Database change detected
2. Worker processes change
3. Update sent through `updates` channel
4. Client applies update to local state

### 3. ScrollerWorker (Internal)

**Purpose:** Core engine that orchestrates data fetching, caching, and update generation.

**Architecture:**
- Runs two async tasks:
  1. **Ordered operations task**: Processes commands sequentially (fetch, refresh, filter changes)
  2. **Unordered operations task**: Handles database callbacks and queries concurrently

**Key Responsibilities:**

#### Command Processing
- **FetchMore**: Sync next page, wait for network if needed
- **FetchNew**: Sync new items from server
- **Refresh/ForceRefresh**: Re-read database and diff with current state
- **GetItems**: Return current cached items
- **Change{Filter,Label,Include,Keywords}**: Modify source state and reset
- **Clear**: Clear pagination cursor

#### Smart Update Generation
The `calculate_scroller_update()` function uses a **prefix-suffix diff algorithm**:

```
Old: [1, 2, 3, 4, 5]
New: [1, 2, 6, 4, 5]

1. Find common prefix: [1, 2] (length=2)
2. Find common suffix: [4, 5] (length=2)
3. Result: ReplaceRange { from: 2, to: 3, items: [6] }
```

This minimizes data transfer and UI updates.

#### State Management
- **items**: Cached list of visible items (synchronized RwLock)
- **task**: Current fetch operation (pagination task)
- **execute_on_online**: Pending operation awaiting network
- **alternative_labels**: Label switching for include filter

#### Error Handling
- Network failures → schedules retry when online
- Task cancellation → graceful abort
- Source errors → propagated as Error updates

### 4. MailScrollerSource Interface

**Purpose:** Abstraction over different data sources (label scrolling, search, etc.)

**Core Methods:**

#### Initialization
```rust
async fn initialize(&mut self, ctx: &Arc<MailUserContext>, invalidation_sender: flume::Sender<()>) -> Result<MailPaginatorJoinHandle, MailContextError>;
```
Sets up the source, returns initial sync task.

#### Pagination
```rust
async fn sync_next(&mut self, ctx: &Arc<MailUserContext>) -> Result<(Vec<Self::Item>, MailPaginatorJoinHandle), MailContextError>;
async fn sync_new(&mut self, ctx: &Arc<MailUserContext>) -> Result<MailPaginatorJoinHandle, MailContextError>;
```
- `sync_next`: Fetch next page, returns items + background task
- `sync_new`: Sync new items from server (e.g., pull to refresh)

#### State Queries
```rust
async fn visible_items(&self, ctx: &Arc<MailUserContext>) -> Result<Vec<Self::Item>, MailContextError>;
async fn all_total(&self, ctx: &Arc<MailUserContext>) -> Result<u64, MailContextError>;
async fn seen_total(&self, ctx: &Arc<MailUserContext>) -> Result<u64, MailContextError>;
async fn synced_total(&self, ctx: &Arc<MailUserContext>) -> Result<u64, MailContextError>;
async fn has_more(&self, ctx: &Arc<MailUserContext>) -> Result<bool, MailContextError>;
```

#### State Mutations
```rust
async fn change_state(&mut self, ctx: &Arc<MailUserContext>, unread: Option<ReadFilter>, label: Option<LocalLabelId>, keywords: Option<SearchOptions>) -> Result<MailPaginatorJoinHandle, MailContextError>;
async fn clear(&mut self, ctx: &Arc<MailUserContext>) -> Result<MailPaginatorJoinHandle, MailContextError>;
```

#### Database Watching
```rust
fn watched_tables(&self) -> Vec<&'static str>;
```
Returns list of database tables to observe for changes.

## Data Source Implementations

### DataScrollerSource<T: ScrollData>

**Use Case:** Standard label-based scrolling (inbox, sent, archive, etc.)

**Backed By:**
- `RemoteConversationScrollerSource` for conversations
- `RemoteMessageScrollerSource` for messages

**Features:**
- Cursor-based pagination
- Read/unread filtering
- Configurable sort order (Time/SnoozeTime)
- Automatic sync from remote API

**State:**
- `label`: Current label ID
- `unread`: Read filter (All/Read/Unread)
- `page_size`: Items per page
- `order_dir`: Sort direction (Asc/Desc)
- `order_field`: Sort field (Time/SnoozeTime)

### SearchScrollerSource

**Use Case:** Search results scrolling

**Features:**
- Keyword-based filtering
- Works with search API endpoints
- No cursor persistence (always fresh search)

**State:**
- `label`: Base label for search scope
- `options`: Search keywords and parameters
- `page_size`: Results per page

## Command Flow Example

### Scenario: User scrolls to bottom and triggers `fetch_more()`

```
1. User calls scroller.fetch_more()
   └─→ Sends FetchMore command to ordered_command channel

2. ScrollerWorker receives command in ordered task
   └─→ Calls fetch_more() method

3. fetch_more() implementation:
   a. Calls sync_next() on source
   b. Source queries database for next page
   c. If items found → return Append update
   d. If empty but has_more → wait for network, schedule retry
   e. If empty and no more → return None update

4. Worker sends ScrollerListUpdate::Append through updates channel

5. Client receives update via MailScrollerHandle
   └─→ Appends items to UI list
```

### Scenario: Database change detected (e.g., new email arrives)

```
1. Database table updated
   └─→ MailScrollerWatcher triggered

2. Watcher sends notification to db_receiver channel

3. ScrollerWorker's unordered task receives notification
   └─→ Sends Refresh command to ordered_command channel

4. Ordered task processes Refresh:
   a. Calls visible_items() on source (reads all visible items from DB)
   b. Runs calculate_scroller_update(old_items, new_items)
   c. Diff algorithm determines minimal update (e.g., ReplaceBefore)
   d. Updates internal cache

5. Worker sends update through updates channel

6. Client receives specific update (e.g., ReplaceBefore { idx: 0, items: [new_email] })
   └─→ UI inserts new email at top of list
```

## Key Design Patterns

### 1. Command-Query Separation
- **Commands** (fetch_more, refresh) → go through channels, processed async
- **Queries** (total, has_more) → use oneshot channels for request-response

### 2. Ordered vs Unordered Operations
- **Ordered**: State-changing operations that must be sequential
- **Unordered**: Queries and reactive updates that can run concurrently

### 3. Optimistic Caching
- Maintains local `items` cache
- Updates incrementally on database changes
- Full refresh only when explicitly requested

### 4. Network Resilience
- Operations gracefully handle offline state
- Automatically retries when connection restored
- Clear error propagation to clients

### 5. Minimal Updates
- Smart diff algorithm minimizes data transfer
- UI receives precise instructions (append, replace range, etc.)
- Reduces rendering overhead

## Threading Model

- **Main Thread**: Creates MailScroller, calls public methods
- **Worker Task 1 (Ordered)**: Processes state-changing commands sequentially
- **Worker Task 2 (Unordered)**: Handles queries and reactive updates
- **Background Tasks**: Network operations, database queries (spawned as needed)

All communication via lock-free channels (flume).

## Error Handling Strategy

- **Recoverable Errors**: Network failures, temporary database locks
  - Returned as `Error` updates
  - Retry mechanisms in place

- **Fatal Errors**: Context dropped, channel closed
  - Triggers worker shutdown
  - Resources cleaned up via Drop impl

- **Partial Failures**: Some items failed to load
  - Returns available items
  - Logs errors, continues operation

## Performance Considerations

- **Lazy Loading**: Only fetches pages as needed
- **Incremental Updates**: Minimal data transfer on changes
- **Database Watching**: Reactive updates without polling
- **Task Cancellation**: Aborts outdated operations immediately
- **Lock-Free Reads**: Items cache uses RwLock for concurrent reads
- **Batched Commands**: Deduplicates rapid successive commands

## Usage Example

```rust
// Create scroller for inbox
let (scroller, mut handle) = MailScroller::conversations(
    ctx_weak,
    inbox_label_id,
    50, // page_size
).await?;

// Fetch first page
scroller.fetch_more(None)?;

// Listen for updates
tokio::spawn(async move {
    while let Ok(update) = handle.updates.recv_async().await {
        match update {
            ScrollerUpdate::List(ScrollerListUpdate::Append { items, .. }) => {
                ui.append_items(items);
            }
            ScrollerUpdate::List(ScrollerListUpdate::ReplaceFrom { idx, items, .. }) => {
                ui.replace_from(idx, items);
            }
            // ... handle other updates
        }
    }
});

// Later: switch to unread filter
scroller.change_filter(ReadFilter::Unread)?;

// Pull to refresh
scroller.fetch_new()?;
```

## Testing Strategy

### Unit Tests: Diff Algorithm

The `calculate_scroller_update` function has 37 test cases covering:
- All update variants (None, Append, ReplaceFrom, ReplaceBefore, ReplaceRange)
- Edge cases: empty lists, single items, complete replacements
- Complex scenarios: mixed insertions, deletions, replacements
- Verification: Each test applies the update and confirms correctness

### Acceptance Tests: End-to-End Scenarios

#### Conversation Scroller (`conversation_mail_scroller.rs`)

**Online/Offline Behavior:**
- **Cached data reads**: Scroller reads from local database when cursor exists, respects visible range
- **Online first page**: Fetches from API when no cache, stores result
- **Multi-page pagination**: Cursor progresses as pages load, syncs next page when cache exhausted
- **Offline with cache**: Serves cached pages until exhausted, then returns network error
- **Insufficient first page**: Automatically merges pages when cache < page_size, requests API to fill gap
- **Recovery from offline**: Schedules fetch_more when network restored, updates data automatically

**Error Handling:**
- **API forbidden (403)**: Returns error, allows retry after server recovery
- **Network timeout**: Returns "No connection" error, queues retry on network restoration
- **Missing items**: Returns empty update when API returns 0 items despite counter showing more

**Database Reactivity:**
- **Small totals (< page_size)**: Refresh triggers automatic fetch_more to ensure data loaded
- **Large totals (≥ page_size)**: Refresh only updates visible items, user scrolls to fetch more
- **New conversation via event**: Detects database change, refreshes scroller, inserts new item at top
- **Modified conversation**: Refresh calculates minimal update (ReplaceBefore/ReplaceRange)

**Special Ordering:**
- **Snoozed conversations**: Orders by snooze_time DESC, then context_time DESC, then display_order DESC
- **Same snooze_time**: Falls back to context_time for tie-breaking
- **Cursor progression**: Correctly handles complex multi-field ordering in pagination

**State Management:**
- **fetch_new()**: Syncs new items from server, sends None then ReplaceBefore update
- **change_label()**: Clears cache, fetches first page of new label
- **change_include()**: Switches between AlmostAllMail ↔ AllMail labels
- **Stale data (Inbox)**: Accepts stale data in Inbox (shows immediately)
- **Stale data (Trash/Spam)**: Rejects stale data in Trash/Spam (returns None, waits for fresh)
- **End cursor invalid**: Detects cursor pointing to deleted item, clears cursor, requests first page

**Race Conditions:**
- **Duplicate cache/API data**: Prevents duplicate Append when cache matches API response exactly
- **Rapid database updates**: Deduplicates refresh commands, processes only once
- **First page sync timing**: Handles invalidation during initial sync without duplication

#### Message Scroller (`message_mail_scroller.rs`)

**Basic Functionality:**
- **Cached reads**: Same behavior as conversations
- **Online pagination**: Cursor progresses as pages load, same strategy as conversations
- **Database notifications**: Listens to message table changes, triggers refresh, updates UI

**Ascending Order:**
- **Scheduled messages**: Displays in ASC order (oldest first) unlike default DESC
- **Order field**: Respects ScrollOrderField configuration per label

#### Search Scroller (`search_mail_scroller.rs`)

**Search-Specific Behavior:**
- **No database watching**: Does NOT refresh on new messages (search is API-only)
- **Modified messages only**: Refreshes when existing search results change
- **No cursor persistence**: Each new search starts fresh (no cache reuse)
- **Keyword changes**: Clears results, fetches new search from API
- **Include filter support**: Works with AlmostAllMail ↔ AllMail switching

**State Transitions:**
- **Multiple rapid changes**: Handles multiple change_include() or change_keywords() in succession
- **Command deduplication**: Ordered task queue deduplicates identical consecutive commands
- **Update sequence**: Each change produces ReplaceFrom { idx: 0 } update

### Edge Cases by Component

#### ScrollerWorker

**Command Queue Management:**
- **Ordered vs unordered tasks**: Ordered task processes state changes sequentially, unordered handles queries concurrently
- **Command deduplication**: `.dedup()` on command queue prevents redundant operations
- **Drain optimization**: Drains all pending commands at once, processes after deduplication

**Network Resilience:**
- **Offline detection**: Checks `network_monitor_service.is_os_offline()` before API calls
- **Wait for online**: Spawns task to `wait_until_online()` then schedules fetch_more
- **Auto-abort**: Aborts pending online-wait task when data arrives from cache

**Refresh Logic:**
- **Force vs normal refresh**: Force always does ReplaceFrom{0}, normal calculates diff
- **Visible items query**: Reads all visible items from DB up to end cursor
- **First page guarantee**: Calls `try_fetch_first_page()` after refresh if seen < page_size

**Task Management:**
- **Task cancellation**: Aborts previous pagination task on filter/label change
- **Task completion**: Awaits task before starting next to avoid race conditions
- **No task + offline**: Returns error immediately (won't make progress)

#### DataScrollerSource (via RemoteConversationScrollerSource / RemoteMessageScrollerSource)

**Initialization:**
- **Cursor detection**: Checks for existing ScrollData in DB
- **Cursor validation**: Verifies end element exists, clears cursor if missing
- **State determination**: Sets Online (with cursor) or NotSynced (without cursor)

**Cursor-Based Pagination:**
- **Request types**: First page (no cursor), previous page (background), next page (conditional)
- **Cache building**: Stores pages in DB and keeps ScrollData Cursor to them for offline use
- **Cursor update**: Moves end cursor forward as pages sync, moves current cursor forward as pages are loaded

**sync_next Strategy:**
- **Cache first**: Checks synced_total > seen_total, serves from DB if available
- **API fallback**: Requests next page from API when cache exhausted
- **Page merging**: Joins incomplete last page with next page for better UX

**Synced vs Seen:**
- **synced_total()**: Count of items synced from API (have remote_id in DB)
- **seen_total()**: Count of items returned to scroller (cursor position)
- **all_total()**: Total count of items in label (from counters)

**State Transitions:**
- **NotSynced → Online**: After first successful API sync creates cursor
- **Online → NotSynced**: When cursor is deleted from DB
- Both states use CachedScrollData; NotSynced reads all items unordered, Online respects cursor boundaries

**Initialization Logic:**
- **With cursor (Online state)**:
  - Validates cursor points to existing element in DB
  - Previous page: syncs in background (fills gaps from new items)
  - Next page: syncs in task only if no cached next page AND total > page_size
  - Invalid cursor: deletes cursor, falls through to NotSynced initialization
- **Without cursor (NotSynced state)**:
  - Empty label (total == 0): returns no task during sync_next
  - Has local data: first page syncs in background with invalidate channel
  - No local data: first page syncs in foreground task (blocks)
- **Invalidate channel**:
  - Notifies scroller when data order changes (NotSynced → Online transition)
  - Passed to background sync tasks (first page, previous page)
  - Triggers refresh in ScrollerWorker to recalculate visible items
  - Used when replace=true in sync_next (local items not in API order)
- **Stale handling**: Label-specific (Inbox accepts, Trash/Spam rejects)

#### SearchScrollerSource

**API-Only Behavior:**
- **No cursor persistence**: Always starts from beginning (display_order = 0)
- **No cache**: Never serves from DB, always hits API
- **Search index**: Uses SearchScrollData table as temporary index (cleared on new search)
- **Progressive results**: Appends to SearchScrollData table as pages arrive

**Invalidation Strategy:**
- **Modified items only**: Watches SearchScrollData table for changes to existing results
- **No new item tracking**: Ignores new messages added to DB
- **Visible items**: Reads from SearchScrollData, not from Message table directly

**Keyword/Include Changes:**
- **Clear on change**: Deletes all SearchScrollData, starts new search
- **Immediate reset**: No transition period, complete clear then fetch

**Future Considerations (TODO):**
- **Encrypted content search**: Current implementation only searches server-indexed metadata (subject, sender, etc.). Future encrypted content search will require:
  - Integrating encrypted index engine with index shared across different client applications

### Remote vs Cached Sources

#### Remote Source (RemoteConversationScrollerSource / RemoteMessageScrollerSource)

**Purpose:** Trait implementation for `ConversationScrollData` and `MessageScrollData` that fetches pages from API and updates DB cursor

**Key Methods:**
- `sync_first_page()`: Fetch first page (no anchor), spawns as background task
- `sync_next_page()`: Fetch next page (anchor=last_element), spawns as background task
- `sync_previous_page()`: Fetch previous page (anchor=first_element, reversed order), spawns as background task

**API Request Pattern:**
- **First page**: No anchor/anchor_id, uses `desc` and `sort` params for ordering
- **Next page**: Uses `anchor` (timestamp) + `anchor_id` (remote_id) from last element, `page_size + 1`
- **Previous page**: Uses `anchor` + `anchor_id` from first element, reversed `desc`, `page_size + 1`
- **Response cleanup**: Removes duplicate anchor element if returned, pops excess if API returns more than requested

**Cursor Building (ScrollData):**
- End cursor is ONLY produced by API response data
- Cursor points to LAST element in each API response
- Updated via `update_scroller_data()` when `update_scroller == true`
- First page: sets cursor (`update_scroller=true`)
- Next page: updates cursor (`update_scroller=true`)
- Previous page: does NOT update cursor (`update_scroller=false`)
- Cursor stores: `remote_id`, `time`/`context_time`, `snooze_time`, `display_order`, `order_dir`, `order_field`

**Database Writes (`quiet_tx`):**
- All DB saves wrapped in `tether.quiet_tx()` to prevent UI notifications
- Rationale: Background-fetched items should not trigger UI updates until visible
- Saves conversations/messages via `create_or_get_local()` inside quiet transaction
- ScrollData cursor updates also inside quiet transaction

**Stale Data Handling:**
- **Trash & Spam ONLY**: Reject stale=true responses (return empty vec)
- **All other labels**: Accept stale=true responses (save and return items)
- **Rationale**: After `delete_all` action in trash/spam, avoid re-loading items that were just deleted. Stale responses would show items that no longer exist on server.
- Checked at: first page, next page, previous page (all three methods)

**Task Spawning:**
- All three methods return `Option<JoinHandle>` (background task)
- Spawned via `ctx.spawn(async move { ... })`
- Caller can await the task or let it run in background
- Invalidate channel: passed to first_page and previous_page, sends notification when items are fetched

#### CachedScrollData (used by both Online and NotSynced states)

**Purpose:** In-memory pagination cursor for buffered database reads

**Online state behavior:**
- Reads within cursor boundaries (respects synced range)
- Progresses cursor as items are consumed
- Updates end cursor as new pages sync

**NotSynced state behavior:**
- Cursor set to read all available items (no boundaries)
- Used when no API-synced cursor exists
- Items may not be in API order

**Operations:**
- `fetch_more()`: Reads next page from DB, advances cursor
- `visible_elements()`: Returns all items within cursor range
- `has_more()`: Checks if more items available within cursor
- `update()`: Refreshes end cursor from DB

#### ScrollData Trait & Database Models

**Purpose:** Generic abstraction over conversation and message scroll persistence

**Database Models:**
- `ConversationScrollData`: Stores cursor position for conversation lists
  - Schema: `mail_conversation_scroll_data` table
  - Primary Key: (local_label_id, unread, scroll_order) - evolved from (local_label_id, unread) in v023
  - Fields:
    - `id`: Optional integer (added v027), used for save/update logic
    - `local_label_id`: Foreign key to labels.local_id (CASCADE DELETE)
    - `unread`: ReadFilter enum (All/Read/Unread)
    - `remote_conversation_id`: API conversation ID
    - `conversation_time`: Unix timestamp for ordering
    - `snooze_time`: Unix timestamp (added v040, 0 = not snoozed)
    - `display_order`: API-provided order value (tie-breaker)
    - `order_dir`: ScrollOrderDir (Asc/Desc) - stored as scroll_order in DB
    - `order_field`: ScrollOrderField (Time/SnoozeTime)

- `MessageScrollData`: Stores cursor position for message lists
  - Schema: `mail_message_scroll_data` table
  - Primary Key: (local_label_id, unread, scroll_order) - evolved from (local_label_id, unread) in v023
  - Fields:
    - `id`: Optional integer (added v027), used for save/update logic
    - `local_label_id`: Foreign key to labels.local_id (CASCADE DELETE)
    - `unread`: ReadFilter enum (All/Read/Unread)
    - `remote_message_id`: API message ID
    - `message_time`: Unix timestamp for ordering
    - `snooze_time`: Unix timestamp (added v040, 0 = not snoozed)
    - `display_order`: API-provided order value (tie-breaker)
    - `order_dir`: ScrollOrderDir (Asc/Desc) - stored as scroll_order in DB
    - `order_field`: ScrollOrderField (Time/SnoozeTime)

**Key Constraint:** Only one cursor per (label, unread filter, order direction) combination

**Future Considerations:**
- **Primary key design flaw**: Including `scroll_order` (order_direction) in primary key was a mistake
- **Recommended approach for new orderings**:
  - Default order direction should be read from label configuration
  - Custom sorting should be in-memory only via `sort(ScrollerSortingOpt)` method
  - Database cursor should only persist position, not sorting preferences
- **Rationale**:
  - Multiple cursors per label waste storage (e.g., both ASC and DESC cursors)
  - Sorting is a view concern, not a persistence concern
  - Changing sort order should not invalidate cached cursor position
  - In-memory sorting allows dynamic user preferences without DB writes

**Trait Methods:**
- `query()`: Generates SQL for fetching items beyond cursor position
- `total()`: Returns total count for label from counters table
- `convert()`: Transforms database models to scroller items (e.g., Conversation → ContextualConversation)
- `into_scroll_data()`: Creates ScrollData from item (reverse conversion)
- `watched_tables()`: Lists tables to observe for changes

#### Storing API Items: `create_or_get_local`

**Purpose:** Safe storage of API responses, preventing duplicates in multiprocess environments

**Methods:**
- `Message::create_or_get_local(&mut self, bond: &Bond<'_>) -> Result<(), StashError>`
- `Conversation::create_or_get_local(&mut self, current_label_id: &LabelId, bond: &Bond<'_>) -> Result<(), StashError>`

**Message behavior:**
1. Check if item with `remote_id` already exists in DB
2. If exists: replace `self` with existing item (no save)
3. If not exists: save new item to DB
4. Mutates `self` to ensure consistent state (either existing or newly saved)

**Conversation behavior:**
1. Check if conversation with `remote_id` already exists
2. If exists and `is_known == true`:
   - Compare label stats for `current_label_id` between new and existing conversation
   - If stats equal (or label not present in both): skip save, return existing
   - If stats differ: update existing (keep local_id, save new data)
3. If exists but `is_known == false` (unknown placeholder):
   - Update with API data (keep local_id, save new data)
4. If not exists: save new conversation to DB

**Key differences:**
- **Message**: Simple remote_id check, no special cases
- **Conversation**: Complex logic for unknown conversations and label stats comparison
  - **Unknown conversations**: Created as placeholders in message view mode when we have messages but no conversation metadata yet
  - **Label stats check**: Avoids overwriting when API returns same data (num_messages, num_unread, time, size, etc. unchanged for given label)

**Why this matters:**
- Mail app is multiprocess (event loop, user actions, background sync can run concurrently)
- Without this check: duplicate rows with same remote_id but different local_ids
- After this check: single source of truth per remote_id, consistent local_id across processes
- Race condition safe: DB query + conditional insert happen in same transaction

**Usage in scroller:**
- `RemoteSource` implementations call `create_or_get_local()` after fetching from API
- Ensures items returned to scroller have stable local_ids
- Cursor persistence works correctly (local_id doesn't change across fetches)

**Query Building:**
The `query()` method generates complex SQL with cursor constraints:
```sql
-- Time-based ordering example:
WHERE context_time > cursor_time
   OR (context_time = cursor_time AND display_order >= cursor_display_order)
ORDER BY context_time DESC, display_order DESC

-- SnoozeTime ordering example (multi-level):
WHERE snooze_time > cursor_snooze_time
   OR (snooze_time = cursor_snooze_time AND context_time > cursor_time)
   OR (snooze_time = cursor_snooze_time AND context_time = cursor_time
       AND display_order >= cursor_display_order)
ORDER BY snooze_time DESC, context_time DESC, display_order DESC
```

**ScrollCursor<T: ScrollData>:**
- Generic cursor containing position parameters (time, snooze_time, display_order, order_dir, order_field)
- Created from ScrollData models via `Into<ScrollCursor<T>>` trait
- Used to query items via `visible_elements()` and `seen_count()`
- Represents either absolute beginning/end or specific position

**CachedScrollData<T: ScrollData>:**
- Generic wrapper holding two cursors: `cursor` (current position) and `end` (synced boundary)
- Works with any type implementing ScrollData trait
- State-agnostic: used by both Online (bounded) and NotSynced (unbounded) states
- Type parameter resolves to `CachedScrollData<ConversationScrollData>` or `CachedScrollData<MessageScrollData>`

**Relationship Flow:**
```
API Response → Conversation/Message (DB models)
                     ↓
            ConversationScrollData/MessageScrollData (persisted cursor)
                     ↓
            ScrollCursor<T> (in-memory position)
                     ↓
            CachedScrollData<T> (pagination state)
                     ↓
            Queries DB → ContextualConversation/Message (scroller items)
```

### State Machine & Command Processing

#### MailScroller Facade: User-Driven Progression

**Design Philosophy:**
- Scroller never progresses without explicit user action
- User calls facade methods (`fetch_more()`, `refresh()`, etc.) → sends commands to worker
- Worker processes commands sequentially, returns updates via channel
- Updates flow back to UI: `ScrollerListUpdate` (Append/ReplaceFrom/None) or `ScrollerStatusUpdate` (errors, fetch status)

**Command Channels:**

**Unordered Channel (`command_receiver`):**
- **Purpose**: Read-only queries, executed immediately in parallel
- **Commands**: `GetTotal`, `GetSeen`, `GetSynced`, `HasMore`, `Cursor`
- **Behavior**: Responds via oneshot channel, doesn't modify state
- **No batching**: Each command processed independently

**Ordered Channel (`ordered_command_sender`):**
- **Purpose**: Mutable operations, must execute sequentially
- **Commands**: `FetchMore`, `FetchNew`, `Refresh`, `ForceRefresh`, `ChangeFilter`, `ChangeLabel`, `ChangeInclude`, `ChangeKeywords`, `Clear`, `GetItems`
- **Batching**: Uses `drain()` + `dedup()` to batch pending commands
- **Deduplication**: Removes redundant commands (e.g., multiple consecutive `Refresh` → single `Refresh`)
- **Sequential**: Each command completes before next starts

**Queue Observation (Triggers Auto-Commands):**

**Database Observer (`db_receiver`):**
- Watches tables via `watched_tables()` (conversations, messages, labels, etc.)
- When DB changes: sends `Refresh(ScrollerSource::Database)` to ordered channel
- Recalculates visible items from updated DB state

**Invalidation Observer (`invalidation_receiver`):**
- Receives from invalidate channel passed to background sync tasks
- Triggered when: NotSynced → Online transition, or order changes detected
- Sends `Refresh(ScrollerSource::Invalidation)` to ordered channel

**Foreground Event Observer (`OnEnterForegroundEvent`):**
- Subscribes to app entering foreground (from background/suspended state)
- When triggered: sends `FetchNew { propagate_status_updates: true }` to ordered channel
- Purpose: Sync new items after user returns to app (may have received items while backgrounded)

**Force Poll Event Observer (`OnForceEventPollEvent`):**
- Subscribes to forced event poll events (manual refresh triggers)
- When triggered: sends `FetchNew { propagate_status_updates: false }` to ordered channel
- Purpose: Sync new items on demand without showing loading bar (e.g., pull-to-refresh already shows UI feedback)

**Why Queue Instead of Instant Execution:**

1. **Abuse Prevention**: Drain + dedup prevents processing 100 rapid `FetchMore` calls
2. **Serialization**: Ordered commands must not run concurrently (e.g., `ChangeLabel` while `FetchMore` in progress)
3. **Batching**: Collect all pending operations, deduplicate, process once
4. **Race Condition Safety**: Sequential execution prevents overlapping cursor updates
5. **Task Coordination**: Ensures previous API task completes before starting next

**Autofire Scenarios (Worker Self-Schedules Commands):**

**1. Empty Fetch with More Available:**
```rust
// In fetch_more(): returned empty but seen < total (has_more_in_label)
// has_more_in_label = label has items we haven't returned yet, but couldn't fetch them
if items.is_empty() && has_more_in_label {
    // Schedule autofire when back online (if not already scheduled)
    spawn(async {
        wait_until_online().await;
        schedule_fetch_more(); // sends FetchMore to ordered channel
    });

    if offline && no_task {
        // Cannot progress without network, return error
        return Err(no_connection);
    } else if online && no_task {
        // Couldn't fetch despite being online (API may not have more)
        warn!("We couldn't sync new items");
        // No autofire, user must call fetch_more() again if desired
    }
    // else: task exists, will complete later
}
```

**2. First Page Not Visible (After Refresh):**
```rust
// At end of refresh(): check if user can see first page
async fn refresh() -> Result<...> {
    // ... recalculate visible items ...

    // Autofire if first page not fully visible
    try_fetch_first_page(src).await?;
    // ↓
    // if total > 0 && seen < page_size && seen < total {
    //     schedule_fetch_more(); // autofire to load first page
    // }
}
```
This ensures after any refresh (DB changes, invalidation, filter/label changes), if the first page isn't fully loaded, it autofires `FetchMore` to fill it.

**Execution Flow Example:**
```
User: scroller.fetch_more()
  ↓
Facade: sends FetchMore to ordered_channel
  ↓
Worker: drains channel → [FetchMore, Refresh, Refresh, FetchMore]
  ↓
Worker: dedup() → [FetchMore, Refresh, FetchMore]
  ↓
Worker: processes sequentially:
  1. FetchMore → sync_next() → append items → send ScrollerListUpdate::Append
  2. Refresh → visible_items() → calculate diff → send ScrollerListUpdate::ReplaceFrom
  3. FetchMore → sync_next() → append items → send ScrollerListUpdate::Append
  ↓
UI: receives 3 updates via updates_channel.recv()
```

**Critical Invariants:**
- Ordered commands never run concurrently
- Unordered commands never modify state
- DB/Invalidation observers always send to ordered channel (never execute directly)
- Autofire always sends to ordered channel (never bypasses queue)
- Worker is single-threaded per scroller instance (one task processing ordered commands)

### MailCursor: Left/Right Swipe Navigation

**Purpose:** Independent view over scroller's item list, enabling left/right swipe navigation through conversations/messages

**Creation:**
```rust
let cursor = scroller.cursor(looking_at_item_id).await?;
```
- Weak reference to parent scroller and items (doesn't prevent scroller from dropping)
- Captures item order at creation time
- Builds navigation state: `prevs` (visited items behind), `curr` (current item), `next` (item ahead)

**Navigation Methods:**

**Peeking (Non-Destructive):**
- `peek_prev()` → `Option<T>`: Returns previous item without moving cursor
- `peek_next()` → `NextMailCursorItem<T>`: Returns next item without moving cursor
  - `None`: No next item exists
  - `Some(T)`: Next item available
  - `Maybe`: Might have next item, needs pagination (call `fetch_next()`)

**Moving:**
- `goto_prev()`: Moves cursor backward, updates state (`curr` → `next`, `prevs.pop()` → `curr`)
- `goto_next()`: Moves cursor forward, updates state (`curr` → `prevs`, `next` → `curr`)
- `fetch_next()`: Calls `parent.fetch_more()`, waits for update, then advances cursor if item available

**Resilience to List Changes:**
- Cursor survives items disappearing (e.g., marking message as read removes from unread filter)
- `peek_prev()`: Walks backward through `prevs` stack, skips missing items until valid item found
- `peek_next()`: Scans forward in items list, skips `prevs` and `curr`, finds first valid item
- State preserved: Even if `curr` item deleted, cursor can still navigate to prev/next

**Use Case Example (Unread Filter):**
1. User swipes through unread messages
2. Viewing message marks it as read → removed from scroller's item list
3. Cursor still remembers position, `peek_next()` returns next unread message
4. User can continue swiping without cursor breaking

**Lifecycle:**
- Multiple cursors can exist for same scroller simultaneously
- Each cursor is independent (one user swiping doesn't affect another cursor)
- Cursor drops when UI view closes (weak references prevent memory leaks)

### Logging & Debuggability

**Scroller Lifecycle Logs:**

```log
INFO  Creating MailScroller id=e10273cd-5acd-4124-8b25-2e22dc1e746e
DEBUG Scroller {id} fetch new after enter foreground
DEBUG Scroller {id} fetch new after force refresh event

INFO  Initializing MailScroller Source
DEBUG Cursor points to end_cursor=Some(...), online=true, total=150, seen=0
DEBUG Syncing previous page in background
DEBUG Syncing next page in a task
```

**Command Execution Logs:**

```log
DEBUG Sending `FetchMore` command uuid=a1b2c3...
TRACE Handling ordered commands: [FetchMore, Refresh(Database)]
TRACE Processing ordered command: FetchMore
DEBUG Fetched next page, items number: 50
TRACE Processed 2 ordered commands

DEBUG Sending `GetTotal` query
DEBUG Sending `GetSeen` query
DEBUG Sending `HasMore` command
```

**State Transitions & Sync:**

```log
INFO  Syncing newest items
DEBUG Syncing previous page in background
DEBUG Cursor points to empty scroll data, will sync first page instead

DEBUG Changed state, new state: Online, initializing...
INFO  Clearing cache for label 1

DEBUG Changing filter to Some(Unread)
DEBUG Changing label to `Inbox`
DEBUG Changing search keywords
```

**Refresh & Update Calculation:**

```log
INFO  Refresh stats - new count: 55, current count: 50

DEBUG Calculating diff...
DEBUG Prefix count: 45
DEBUG Suffix count: 5
DEBUG Append: items number: 10
DEBUG No update required
```

**ScrollerSource Tracking:**

Every update carries `ScrollerSource` indicating origin:
- **`ScrollEvent(Uuid)`**: User/UI-initiated actions (fetch_more, fetch_new, refresh, filter/label changes, etc.)
- **`Database`**: Local DB change detected by watcher (e.g., message marked as read)
- **`Invalidation`**: Background task completed, notified via invalidate channel

```log
TRACE Processing ordered command: Refresh(Database)
INFO  Refresh stats - new count: 55, current count: 50
DEBUG Append: items number: 5, src=Database

TRACE Processing ordered command: FetchMore { src: ScrollEvent(a1b2c3-...) }
DEBUG Fetched next page, items number: 50
DEBUG Append: items number: 50, src=ScrollEvent(a1b2c3-...)
```

**Critical for debugging:**
- **ScrollEvent UUID correlation**: Ties user action to resulting updates (same UUID throughout flow)
- **Database vs ScrollEvent**: Distinguishes reactive updates (DB changes) from proactive (user scroll)
- **Invalidation**: Shows when background tasks complete (NotSynced → Online transition)

**Error Handling:**

```log
WARN  Scroller is offline, will not progress any further
WARN  We couldn't sync new items
WARN  Couldn't initialize scroller, continuing anyway

ERROR Failed to handle ordered command: NotSynced
ERROR Failed to receive db update: channel closed
ERROR Failed to fetch next page: NetworkError
ERROR Error occurred while waiting for previous request: timeout
```

**MailCursor Logs:**

```log
INFO  cursor{id=e10273cd...}:new: Creating MailCursor id=09fb3d44... looking_at=LocalConversationId(962)
INFO  Moving backwards
INFO  Moving forwards
INFO  drop{id=b8cd3cfe...}: Dropping MailCursor
```

**Task Management:**

```log
DEBUG Awaiting for previous task
DEBUG Aborting previous task
DEBUG We do not see the first page, requesting fetch more
DEBUG No items to return, requesting additional fetch more
DEBUG No new items fetched
```

**Debugging Workflow:**

1. **Track scroller lifecycle**: Search logs for `Creating MailScroller` → find scroller UUID
2. **Follow command flow**: Filter by UUID → see ordered command queue processing
3. **Identify bottlenecks**: Look for long gaps between `Sending` and `Processing` logs
4. **Diagnose sync issues**: Check `Initializing MailScroller Source` → state transitions (NotSynced → Online)
5. **Trace refresh cycles**: Follow `Refresh stats` → diff calculation → update type
6. **Monitor cursor behavior**: Filter `cursor{id=...}` spans for navigation issues
7. **Catch errors early**: Search for `WARN`/`ERROR` logs with context spans

**Log Correlation:**
- UUIDs tie commands to execution (e.g., `uuid=a1b2c3...` appears in both `Sending` and `Processing`)
- Scroller ID persists across entire lifecycle
- Tracing spans nest operations: `cursor{id=...}:new` shows cursor creation within existing cursor context
- Transaction IDs (`tx{id=...}`) correlate DB operations with scroller actions

**Performance Profiling:**
- Span durations show operation timing (enabled via tracing subscriber)
- `items number` logs track page sizes and append counts
- `Prefix/Suffix count` logs reveal update efficiency (high prefix = efficient append, low = full replace)
- Background vs foreground task logs identify where waiting occurs

### Test Patterns

**TestScroller<T> Wrapper:**

Generic helper for testing MailScroller (conversations/messages/search). Imperfect but sufficient for robust scroller tests.

**Creation:**

```rust
// Static constructors (wait for initial updates)
TestScroller::conversations(ctx, label_id, page_size).await?
TestScroller::messages(ctx, label_id, page_size).await?
TestScroller::search(ctx, search_options, page_size).await?

// Instant constructors (no initial wait)
TestScroller::conversations_instant(ctx, label_id, page_size).await?
TestScroller::new_instant(scroller, handle)
```

**Action Methods (trigger commands, return immediately):**

```rust
test_scroller.fetch_more()?
test_scroller.fetch_new()?
test_scroller.refresh()?
test_scroller.force_refresh()?
test_scroller.change_filter(ReadFilter::Unread)?
test_scroller.change_label(label_id)?
test_scroller.change_include(IncludeSwitch::On)?
test_scroller.change_keywords(search_options)?
```

**Waiting for Updates:**

```rust
// Blocking wait (until next non-status update arrives)
test_scroller.wait_for_update().await?

// Non-blocking wait with timeout
test_scroller.try_wait_for_update(Duration::from_secs(1)).await?

// Convenience: action + wait
test_scroller.fetch_more_and_wait().await?
test_scroller.fetch_new_and_wait().await?
test_scroller.refresh_and_wait().await?
```

**Assertions:**

```rust
// Blocking: wait for next update and assert type
test_scroller.match_next_update(TestUpdate::Append { items: 50 }).await;

// Non-blocking: assert all collected updates so far
test_scroller.assert_updates(&[
    TestUpdate::ReplaceFrom { idx: 0, items: 50 },
    TestUpdate::Append { items: 50 },
]);

// Non-blocking: assert collected items
test_scroller.assert_items(&expected_items);
```

**TestUpdate Variants:**

```rust
TestUpdate::None
TestUpdate::Append { items: usize }
TestUpdate::ReplaceFrom { idx: usize, items: usize }
TestUpdate::ReplaceBefore { idx: usize, items: usize }
TestUpdate::ReplaceRange { from: usize, to: usize, items: usize }
TestUpdate::Error(String)
```

**Query Methods:**

```rust
test_scroller.items()  // Returns collected items
test_scroller.has_more().await?
test_scroller.total().await?
test_scroller.seen().await?
test_scroller.supports_include_filter().await
```

**Test Data Helpers:**

```rust
// Generate test messages/conversations with shifted order
test_messages(count: usize, order_shift: usize) -> Vec<Message>
test_conversations(count: usize, order_shift: usize) -> Vec<Conversation>

// Save to DB with labels
HashMap<Vec<LabelId>, Vec<Conversation>>.save_to_database(tether).await
HashMap<Vec<LabelId>, Vec<Message>>.save_to_database(tether).await
(label_id, Vec<Message>).save_to_database(tether).await
```

**Common Test Patterns:**

1. **Initial load:**
   ```rust
   let mut scroller = TestScroller::conversations(ctx, label_id, 50).await?;
   // Constructor waits for initial update (if any cached data)
   scroller.assert_updates(&[TestUpdate::ReplaceFrom { idx: 0, items: 50 }]);
   ```

2. **Pagination:**
   ```rust
   scroller.fetch_more_and_wait().await?;
   scroller.match_next_update(TestUpdate::Append { items: 50 }).await;
   ```

3. **Testing no-update scenarios:**
   ```rust
   scroller.fetch_more()?;
   let result = scroller.try_wait_for_update(Duration::from_secs(1)).await?;
   assert!(result.is_none()); // No update within timeout
   ```

4. **Filter changes:**
   ```rust
   scroller.change_filter(ReadFilter::Unread)?;
   scroller.match_next_update(TestUpdate::ReplaceFrom { idx: 0, items: 20 }).await;
   ```

**Limitations:**

- Status updates (FetchNewStart/End) are filtered out automatically
- Update assertions compare counts, not actual item content (use `assert_items()` for content)
- Background tasks complete unpredictably; use timeouts for no-update assertions
- Error updates stored separately but can fail tests unexpectedly

