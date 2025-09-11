use crate::actions::event_poll::EventPoll;
use proton_action_queue::action::{ActionDependencyKey, ActionDependencyKeys};
use std::fmt::Debug;

/// Helper utility to build action dependency chains.
///
/// By default the builder always depends on the event loop action. To circumvent this use
/// the `without_event_loop_dependency` constructor.
#[derive(Debug, Default)]
pub struct ActionDependencyKeysBuilder(ActionDependencyKeys);

impl ActionDependencyKeysBuilder {
    #[must_use]
    pub fn new() -> Self {
        let mut keys = ActionDependencyKeys::default();
        keys.record.push(EventPoll::dependency_key());
        Self(keys)
    }

    #[must_use]
    pub fn without_event_loop_dependency() -> Self {
        Self(ActionDependencyKeys::default())
    }

    #[must_use]
    pub fn with_required(mut self, key: ActionDependencyKey) -> Self {
        self.0.required.push(key);
        self
    }

    #[must_use]
    pub fn with_required_many(
        mut self,
        keys: impl IntoIterator<Item = ActionDependencyKey>,
    ) -> Self {
        self.0.required.extend(keys);
        self
    }
    #[must_use]
    pub fn with_optional(mut self, key: ActionDependencyKey) -> Self {
        self.0.required.push(key);
        self
    }

    #[must_use]
    pub fn with_optional_many(
        mut self,
        keys: impl IntoIterator<Item = ActionDependencyKey>,
    ) -> Self {
        self.0.required.extend(keys);
        self
    }

    #[must_use]
    pub fn record(mut self, key: ActionDependencyKey) -> Self {
        self.0.record.push(key);
        self
    }

    #[must_use]
    pub fn record_many(mut self, keys: impl IntoIterator<Item = ActionDependencyKey>) -> Self {
        self.0.record.extend(keys);
        self
    }
    #[must_use]
    pub fn with_required_ext<T: LocalIdActionDepExt>(mut self, id: T) -> Self {
        self.0.required.push(id.to_dependency_key());
        self
    }

    #[must_use]
    pub fn with_required_many_ext<T: LocalIdActionDepExt>(
        mut self,
        ids: impl IntoIterator<Item = T>,
    ) -> Self {
        self.0
            .required
            .extend(ids.into_iter().map(|id| id.to_dependency_key()));
        self
    }
    #[must_use]
    pub fn with_optional_ext<T: LocalIdActionDepExt>(mut self, id: T) -> Self {
        self.0.required.push(id.to_dependency_key());
        self
    }

    #[must_use]
    pub fn with_optional_many_ext<T: LocalIdActionDepExt>(
        mut self,
        ids: impl IntoIterator<Item = T>,
    ) -> Self {
        self.0
            .required
            .extend(ids.into_iter().map(|id| id.to_dependency_key()));
        self
    }

    #[must_use]
    pub fn record_ext<T: LocalIdActionDepExt>(mut self, id: T) -> Self {
        self.0.record.push(id.to_dependency_key());
        self
    }

    #[must_use]
    pub fn record_many_ext<T: LocalIdActionDepExt>(
        mut self,
        ids: impl IntoIterator<Item = T>,
    ) -> Self {
        self.0
            .record
            .extend(ids.into_iter().map(|id| id.to_dependency_key()));
        self
    }

    /// Record a required dependency key that is related to the current operation.
    ///
    /// An example of this would be assigning a label to a message. We need to depend on the label's
    /// creation and we also need to register that we used the label's id so that a label delete
    /// operation con wait for this action to complete.
    ///
    /// Technically deleting a label should not depend on any previous actions, but to guarantee
    /// the ordering of the operations, deleting a label should run after the last operation
    /// which uses the label.
    #[must_use]
    pub fn with_required_related<T: LocalIdActionDepExt>(mut self, id: T) -> Self {
        self.0.required.push(id.to_create_dependency_key());
        self.0.record.push(id.to_dependency_key());
        self
    }

    /// Record an optional dependency key that is related to the current operation.
    ///
    /// See the details in [`with_required_related`] for more details.
    #[must_use]
    pub fn with_optional_related<T: LocalIdActionDepExt>(mut self, id: T) -> Self {
        self.0.optional.push(id.to_create_dependency_key());
        self.0.record.push(id.to_dependency_key());
        self
    }

    #[must_use]
    pub fn with_required_related_many<T: LocalIdActionDepExt>(
        mut self,
        ids: impl IntoIterator<Item = T>,
    ) -> Self {
        for id in ids {
            self.0.required.push(id.to_create_dependency_key());
            self.0.record.push(id.to_dependency_key());
        }
        self
    }

    #[must_use]
    pub fn with_optional_related_many<T: LocalIdActionDepExt>(
        mut self,
        ids: impl IntoIterator<Item = T>,
    ) -> Self {
        for id in ids {
            self.0.optional.push(id.to_create_dependency_key());
            self.0.record.push(id.to_dependency_key());
        }
        self
    }

    #[must_use]
    pub fn build(self) -> ActionDependencyKeys {
        self.0
    }
}

pub trait LocalIdActionDepExt: Debug + Copy + Clone {
    fn to_dependency_key(&self) -> ActionDependencyKey;

    fn to_create_dependency_key(&self) -> ActionDependencyKey;

    fn to_custom_dependency_key(&self, prefix: &str) -> ActionDependencyKey;
}
