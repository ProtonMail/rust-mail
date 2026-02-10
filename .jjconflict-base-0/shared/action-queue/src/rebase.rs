use std::collections::HashSet;

#[derive(Debug, Default, Clone)]
pub struct RebaseChangeSet {
    set: HashSet<RebaseKey>,
}

impl RebaseChangeSet {
    pub fn add(&mut self, key: impl Into<RebaseKey>) {
        self.set.insert(key.into());
    }

    pub fn add_many(&mut self, keys: impl IntoIterator<Item = impl Into<RebaseKey>>) {
        self.set.extend(keys.into_iter().map(Into::into));
    }

    #[must_use]
    pub fn contains(&self, key: &RebaseKey) -> bool {
        self.set.contains(key)
    }
}

impl<T: Into<RebaseKey>> From<T> for RebaseChangeSet {
    fn from(key: T) -> Self {
        Self {
            set: HashSet::from_iter([key.into()]),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct RebaseKey(String);

impl<T: Into<String>> From<T> for RebaseKey {
    fn from(value: T) -> Self {
        Self(value.into())
    }
}
