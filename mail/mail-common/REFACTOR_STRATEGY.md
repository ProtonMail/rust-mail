# Mobile Actions Refactor Strategy

## Overview

This document outlines the strategy for refactoring the current mobile action system to provide consistent, dynamic action calculation across all contexts (list, message, conversation) with proper naming conventions.

## Current State

### Existing Bottom Bar Actions (List Context)
- **File**: `mail/mail-common/src/actions/available_action/all_bottom_bar_actions.rs`
- **Struct**: `AllBottomBarMessageActions`
- **Fields**: `visible_bottom_bar_actions`, `hidden_bottom_bar_actions`
- **Methods on Message**: `all_available_bottom_bar_actions_for_messages()`, `visible_bottom_bar_actions()`, `hidden_bottom_bar_actions()`
- **Methods on ContextualConversation**: `visible_bottom_bar_actions()`, `hidden_bottom_bar_actions()`

### Existing Message Actions (Static Implementation)
- **File**: `mail/mail-common/src/actions/available_action/message_action.rs`
- **Struct**: `MessageAvailableActions`
- **Implementation**: Static logic in `Message::available_actions()`
- **Fields**: `message_actions`, `reply_actions`, `move_actions`, `general_actions`

### Existing Conversation Actions (Static Implementation)
- **File**: `mail/mail-common/src/actions/available_action/conversation_action.rs`
- **Struct**: `ConversationAvailableActions`
- **Implementation**: Static logic in `Conversation::available_actions()`
- **Fields**: `conversation_actions`, `move_actions`, `general_actions`

## Target State

### Unified Action Pattern
All action contexts will follow the same pattern:
```rust
pub struct All{Context}Actions {
    pub visible_{context}_actions: Vec<{Context}Actions>,
    pub hidden_{context}_actions: Vec<{Context}Actions>,
}
```

### Three Action Contexts
1. **List Actions** (renamed from bottom bar actions)
2. **Message Actions** (dynamic replacement for current static implementation)
3. **Conversation Actions** (dynamic replacement for current static implementation)

## Refactor Strategy

### Phase 1: Rename Bottom Bar → List Actions

**Breaking Changes Allowed**: Complete renaming without backward compatibility.

#### File Changes
```
mail/mail-common/src/actions/available_action/
├── all_bottom_bar_actions.rs → all_list_actions.rs
├── all_message_actions.rs (new)
└── all_conversation_actions.rs (new)

mail/mail-common/src/tests/actions/available_actions/
├── action_bottom_bar.rs → action_list_actions.rs
├── action_message_actions.rs (new)
└── action_conversation_actions.rs (new)
```

#### Core Struct Renaming
```rust
// Before
AllBottomBarMessageActions → AllListActions
BottomBarActions → ListActions

// Fields
visible_bottom_bar_actions → visible_list_actions
hidden_bottom_bar_actions → hidden_list_actions
```

#### Method Renaming
```rust
// On Message
all_available_bottom_bar_actions_for_messages() → all_available_list_actions_for_messages()
visible_bottom_bar_actions() → visible_list_actions()
hidden_bottom_bar_actions() → hidden_list_actions()

// On ContextualConversation
visible_bottom_bar_actions() → visible_list_actions()
hidden_bottom_bar_actions() → hidden_list_actions()
```

### Phase 2: Introduce Dynamic Message Actions

#### New Structure
```rust
// mail/mail-common/src/actions/available_action/all_message_actions.rs

#[derive(Debug, Clone, PartialEq)]
pub struct AllMessageActions {
    pub visible_message_actions: Vec<MessageActions>,
    pub hidden_message_actions: Vec<MessageActions>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MessageActions {
    // Read state
    MarkRead,
    MarkUnread,

    // Star state
    Star,
    Unstar,

    // Organization
    LabelAs,
    MoveTo,
    MoveToSystemFolder(MovableSystemFolderAction),

    // Communication
    Reply,
    ReplyAll,
    Forward,

    // Export/View
    SavePDF,
    Print,
    ViewHeaders,
    ViewHTML,

    // Utility
    ReportPhishing,
    More,
}
```

#### Implementation on Message
```rust
impl Message {
    pub async fn all_available_message_actions_for_message(
        message_id: LocalMessageId,
        tether: &Tether,
    ) -> Result<AllMessageActions, AppError> {
        // Get user settings for message_toolbar
        let message_toolbar_actions = MobileActions::message_toolbar_actions(tether).await?;

        // Calculate visible actions based on settings + message state
        let visible_message_actions = Self::visible_message_actions(
            message_id,
            &message_toolbar_actions,
            tether
        ).await?;

        // Calculate hidden actions (all possible - visible)
        let hidden_message_actions = Self::hidden_message_actions(
            message_id,
            &visible_message_actions,
            tether
        ).await?;

        Ok(AllMessageActions {
            visible_message_actions,
            hidden_message_actions,
        })
    }

    async fn visible_message_actions(
        message_id: LocalMessageId,
        toolbar_actions: &[MobileActions],
        tether: &Tether,
    ) -> Result<Vec<MessageActions>, AppError> {
        // Convert MobileActions to MessageActions based on message state
        // Apply 5-action limit + More button logic
    }

    async fn hidden_message_actions(
        message_id: LocalMessageId,
        visible_actions: &[MessageActions],
        tether: &Tether,
    ) -> Result<Vec<MessageActions>, AppError> {
        // All possible actions for this message - visible actions
    }
}
```

### Phase 3: Introduce Dynamic Conversation Actions

#### New Structure
```rust
// mail/mail-common/src/actions/available_action/all_conversation_actions.rs

#[derive(Debug, Clone, PartialEq)]
pub struct AllConversationActions {
    pub visible_conversation_actions: Vec<ConversationActions>,
    pub hidden_conversation_actions: Vec<ConversationActions>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConversationActions {
    // Read state
    MarkRead,
    MarkUnread,

    // Star state
    Star,
    Unstar,

    // Organization
    LabelAs,
    MoveTo,
    MoveToSystemFolder(MovableSystemFolderAction),
    Snooze,

    // Utility
    More,
}
```

#### Migration from Conversation to ContextualConversation
```rust
// REMOVE from Conversation
impl Conversation {
    // DELETE: available_actions() method
}

// ADD to ContextualConversation
impl ContextualConversation {
    pub async fn all_available_conversation_actions_for_conversations(
        current_label_id: LocalLabelId,
        conversation_ids: Vec<LocalConversationId>,
        tether: &Tether,
    ) -> Result<AllConversationActions, AppError> {
        // Get user settings for conversation_toolbar
        let conversation_toolbar_actions = MobileActions::conversation_toolbar_actions(tether).await?;

        // Calculate visible actions based on settings + conversation states
        let visible_conversation_actions = Self::visible_conversation_actions(
            current_label_id,
            conversation_ids,
            &conversation_toolbar_actions,
            tether
        ).await?;

        // Calculate hidden actions
        let hidden_conversation_actions = Self::hidden_conversation_actions(
            current_label_id,
            conversation_ids,
            &visible_conversation_actions,
            tether
        ).await?;

        Ok(AllConversationActions {
            visible_conversation_actions,
            hidden_conversation_actions,
        })
    }
}
```

### Phase 4: Settings Integration (Already Complete!)

The `MobileActions` implementation is already up to date with proper action lists for each context:

#### Existing MobileActions (No Changes Needed)
```rust
impl MobileActions {
    // EXISTING: Toolbar action methods (keep as-is)
    pub async fn list_toolbar_actions(tether: &Tether) -> Result<Vec<MobileActions>, AppError>
    pub async fn message_toolbar_actions(tether: &Tether) -> Result<Vec<MobileActions>, AppError>
    pub async fn conversation_toolbar_actions(tether: &Tether) -> Result<Vec<MobileActions>, AppError>

    // EXISTING: Action lists (already properly defined - no changes needed!)
    pub fn all_list_actions() -> Vec<MobileActions> {
        // ✅ Already correct: [ToggleRead, Trash, Move, Label, ToggleStar, Snooze, Archive, Spam]
    }

    pub fn all_message_actions() -> Vec<MobileActions> {
        // ✅ Already correct: includes message-specific actions like Reply, Forward, SavePDF, etc.
    }

    pub fn all_conversation_actions() -> Vec<MobileActions> {
        // ✅ Already correct: [ToggleRead, Trash, Move, Label, ToggleStar, Snooze, Archive, Spam]
    }
}
```

**Note**: Phase 4 is essentially complete - the `MobileActions` enum and its methods are already properly structured for our refactor!

### Phase 5: Update Tests & UniFfi Bindings

#### Test Structure
```
mail/mail-common/src/tests/actions/available_actions/
├── action_list_actions.rs (renamed from action_bottom_bar.rs)
├── action_message_actions.rs (new, comprehensive tests)
└── action_conversation_actions.rs (new, comprehensive tests)
```

#### UniFfi Bindings
```
mail/mail-uniffi/src/mail/datatypes/available_action/
├── all_list_actions.rs (renamed from all_bottom_bar_actions.rs)
├── all_message_actions.rs (new)
└── all_conversation_actions.rs (new)
```

## Implementation Timeline

### Sprint 1: List Actions Rename
- [ ] Rename all bottom bar → list terminology
- [ ] Update tests to use new names
- [ ] Update UniFfi bindings
- [ ] Verify mobile app compatibility

### Sprint 2: Dynamic Message Actions
- [ ] Create `AllMessageActions` structure
- [ ] Implement dynamic logic on `Message`
- [ ] Create comprehensive tests
- [ ] Update UniFfi bindings

### Sprint 3: Dynamic Conversation Actions
- [ ] Create `AllConversationActions` structure
- [ ] Migrate logic from `Conversation` to `ContextualConversation`
- [ ] Implement dynamic logic
- [ ] Create comprehensive tests
- [ ] Update UniFfi bindings

### Sprint 4: Integration & Cleanup
- [ ] Remove old static implementations
- [ ] Update documentation
- [ ] Performance testing
- [ ] Mobile app integration testing
- [ ] ✅ Settings integration (already complete - `MobileActions` lists are up to date!)

## Breaking Changes Summary

### API Changes
1. **Struct Names**: `AllBottomBarMessageActions` → `AllListActions`
2. **Enum Names**: `BottomBarActions` → `ListActions`
3. **Method Names**: All `*bottom_bar*` methods → `*list*` methods
4. **Location Changes**: Conversation actions move from `Conversation` to `ContextualConversation`
5. **Return Types**: New `AllMessageActions` and `AllConversationActions` types

### File Changes
1. **Renamed Files**: `all_bottom_bar_actions.rs` → `all_list_actions.rs`
2. **New Files**: `all_message_actions.rs`, `all_conversation_actions.rs`
3. **Test Files**: Corresponding renames and new test files

### Mobile App Impact
1. **UniFfi Bindings**: All action-related bindings need updates
2. **API Calls**: Method names and return types change
3. **UI Updates**: May need updates to handle new action structures

## Benefits

1. **Consistency**: All action contexts follow the same visible/hidden pattern
2. **Flexibility**: Dynamic action calculation based on user settings
3. **Maintainability**: Clear separation of concerns between contexts
4. **Extensibility**: Easy to add new actions or modify behavior
5. **User Experience**: Proper customization for all contexts

## Risk Mitigation

1. **Testing**: Comprehensive test coverage for all contexts
2. **Documentation**: Clear migration guide for mobile teams
3. **Staging**: Deploy to staging environment first
4. **Rollback Plan**: Keep ability to revert if critical issues found
5. **Communication**: Coordinate with mobile teams before implementation

## Success Criteria

1. ✅ All tests pass with new action system
2. ✅ Mobile apps build and function correctly with new UniFfi bindings
3. ✅ User action customization works across all contexts
4. ✅ Performance is equal or better than current implementation
5. ✅ Code is more maintainable and consistent
