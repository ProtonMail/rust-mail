use super::generic_mobile_actions::{ActionContext, GenericMobileActions};
use crate::datatypes::{MobileAction, SystemLabelId};
use proton_core_api::services::proton::LabelId;

pub struct MobileActionsBuilder<T: GenericMobileActions> {
    visible_actions: Vec<T>,
    hidden_actions: Vec<T>,
    context: ActionContext,
    max_visible: usize,
}

impl<T: GenericMobileActions + std::fmt::Debug> MobileActionsBuilder<T> {
    pub fn new(context: ActionContext, mobile_actions: &[MobileAction]) -> Self {
        let mut visible_actions = Vec::new();

        for mobile_action in mobile_actions {
            if let Some(action) = T::from_mobile_action(mobile_action, &context) {
                visible_actions.push(action);
            }
        }

        Self {
            visible_actions,
            hidden_actions: Vec::new(),
            context,
            max_visible: 5, // Default mobile toolbar limit
        }
    }

    pub fn build(mut self) -> (Vec<T>, Vec<T>) {
        if self.visible_actions.len() > self.max_visible {
            self.visible_actions.truncate(self.max_visible);
        }
        self.visible_actions.push(T::more());

        self.add_contextual_hidden_actions();

        (self.visible_actions, self.hidden_actions)
    }

    fn add_contextual_hidden_actions(&mut self) {
        let all_possible = self.get_hidden_actions(&self.visible_actions);

        for action in all_possible {
            if !self.visible_actions.contains(&action) && !self.hidden_actions.contains(&action) {
                self.hidden_actions.push(action);
            }
        }
    }

    /// Get all possible actions for current context, excluding visible actions
    pub fn get_hidden_actions(&self, visible_actions: &[T]) -> Vec<T> {
        let all_actions = self.get_all_possible_actions();

        // Filter out visible actions and their counter-actions
        all_actions
            .into_iter()
            .filter(|action| !self.is_action_or_counter_visible(action, visible_actions))
            .collect()
    }

    /// Check if an action (or its counter-action) is already visible
    /// Takes mixed states into account where both actions should be available
    fn is_action_or_counter_visible(&self, action: &T, visible_actions: &[T]) -> bool {
        // Check if the exact action is already visible
        if visible_actions.contains(action) {
            return true;
        }

        // For counter-actions, only filter out if we're NOT in a mixed state
        // In mixed states some of the actions should have both options available (e.g., some read + some unread).
        for visible in visible_actions {
            if T::are_counter_actions(action, visible) {
                // Don't filter out counter-actions in mixed states
                if self.is_mixed_state_for_action(action) {
                    return false;
                }
                return true;
            }
        }

        false
    }

    fn is_mixed_state_for_action(&self, action: &T) -> bool {
        let read_actions = [T::mark_read(), T::mark_unread()];
        if read_actions
            .iter()
            .any(|a| std::mem::discriminant(a) == std::mem::discriminant(action))
        {
            return self.context.any_unread && !self.context.all_read;
        }

        let star_actions = [T::star(), T::unstar()];
        if star_actions
            .iter()
            .any(|a| std::mem::discriminant(a) == std::mem::discriminant(action))
        {
            return self.context.any_starred && !self.context.all_starred;
        }

        false
    }

    pub fn get_all_possible_actions(&self) -> Vec<T> {
        let mut actions = Vec::new();

        let has_items = self.context.any_unread || self.context.all_read;

        if has_items {
            if self.context.all_read {
                actions.push(T::mark_unread());
            } else if self.context.any_unread && !self.context.any_read {
                actions.push(T::mark_read());
            } else if self.context.any_unread && self.context.any_read {
                actions.push(T::mark_read());
                actions.push(T::mark_unread());
            }

            if self.context.any_starred && !self.context.all_starred {
                actions.push(T::star());
                actions.push(T::unstar());
            } else if self.context.all_starred {
                actions.push(T::unstar());
            } else if !self.context.any_starred {
                actions.push(T::star());
            }
        }

        actions.push(T::move_to());

        actions.extend(T::get_high_priority_actions(&self.context));

        actions.push(T::label_as());

        if self.context.current_label == LabelId::spam() {
            actions.push(T::not_spam(self.context.folders.inbox));
        }

        // System folder actions - consistent ordering: Inbox, Archive, Spam, Trash
        // Only include folders we're not currently in

        // 1. Inbox (when in archive or trash)
        if [LabelId::archive(), LabelId::trash()].contains(&self.context.current_label) {
            actions.push(T::move_to_system_folder(self.context.folders.inbox));
        }

        // 2. Archive (when not in archive)
        if self.context.current_label != LabelId::archive() {
            actions.push(T::move_to_system_folder(self.context.folders.archive));
        }

        // 3. Spam (when not in spam or trash)
        if ![LabelId::spam(), LabelId::trash()].contains(&self.context.current_label) {
            actions.push(T::move_to_system_folder(self.context.folders.spam));
        }

        // 4. Trash (when not in trash or spam)
        if ![LabelId::trash(), LabelId::spam()].contains(&self.context.current_label) {
            actions.push(T::move_to_system_folder(self.context.folders.trash));
        }
        if [LabelId::trash(), LabelId::spam()].contains(&self.context.current_label) {
            actions.push(T::permanent_delete());
        }

        // Type-specific actions come after system folders
        actions.extend(T::get_low_priority_actions(&self.context));

        actions
    }
}
