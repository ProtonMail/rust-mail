use std::collections::{HashMap, VecDeque};
use std::ops::ControlFlow;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::*;
use crate::blob::{Blob, BlobId, RevisionEvent};
use crate::entry::EntryValue;
use crate::index::extensions::WithMut;
use crate::index::prelude::{IndexStore, IndexStoreEvent, IndexStoreOperation};

mod bools;
mod ints;
mod tags;

impl<V> IndexStore for Trivial<V>
where
    V: Serialize + for<'de> Deserialize<'de>,
    V: IndexableValue,
    for<'a> &'a EntryValue: TryInto<V>,
    EntryValue: From<V>,
{
    fn id(&self) -> &str {
        Self::name()
    }
    fn write(
        &self,
        revision: Option<BlobId>,
        operations: &[IndexStoreOperation],
    ) -> Box<dyn Send + Iterator<Item = IndexStoreEvent>> {
        let write = Writer::new(self, revision, operations);
        Box::new(write)
    }
}

struct Writer<V>
where
    V: IndexableValue,
{
    operations: VecDeque<IndexStoreOperation>,
    iterating: VecDeque<IndexStoreEvent>,
    modified: bool,
    blob: Option<Blob<Index<V>>>,
    index: Option<Index<V>>,
}

impl<V> Writer<V>
where
    V: IndexableValue + for<'de> Deserialize<'de>,
{
    fn new(
        _index: &Trivial<V>,
        revision: Option<BlobId>,
        operations: &[IndexStoreOperation],
    ) -> Self {
        // TODO: we could batch multiple subsequent removals or inserts here to avoid multiple loops, keeping it simple now
        Self {
            operations: operations.iter().cloned().collect(),
            iterating: [].into(),
            modified: false,
            blob: revision.map(|revision| Blob::new(revision, Trivial::<V>::name())),
            index: None,
        }
    }
}

impl<V> Iterator for Writer<V>
where
    V: Serialize + for<'de> Deserialize<'de>,
    V: IndexableValue,
    for<'a> &'a EntryValue: TryInto<V>,
    EntryValue: From<V>,
{
    type Item = IndexStoreEvent;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(next) = self.iterating.pop_front() {
                return Some(next);
            }

            let index = match &mut self.index {
                Some(index) => index,
                None => match self.blob.as_mut().map(Blob::fetch_and_free) {
                    Some(ControlFlow::Break(load)) => return Some(IndexStoreEvent::Load(load)),
                    Some(ControlFlow::Continue(cached)) => {
                        self.index.insert(cached.as_ref().clone())
                    }
                    None => self.index.insert(Default::default()),
                },
            };

            if let Some(operation) = self.operations.pop_front() {
                match operation {
                    IndexStoreOperation::Insert(entry, attr, values) => {
                        let inserted = insert(index, entry, attr, values.as_ref());
                        self.modified |= inserted;
                        self.iterating
                            .push_back(IndexStoreEvent::Inserted { entry, attr })
                    }
                    IndexStoreOperation::Remove(entry) => {
                        let removed = remove(index, entry);
                        self.modified |= !removed.is_empty();
                        self.iterating.extend(removed)
                    }
                }
            } else {
                // no more operations to perform, wrap up
                if std::mem::take(&mut self.modified) {
                    let index = std::mem::take(index);
                    let id = BlobId::generate(&index);
                    let (release, save) = self
                        .blob
                        .get_or_insert_with(|| Blob::new(id, Trivial::<V>::name()))
                        .save_and_free(id, Arc::new(index));
                    self.iterating.push_back(IndexStoreEvent::Save(save));
                    self.iterating.extend(release.map(IndexStoreEvent::Release));
                    self.iterating
                        .push_back(IndexStoreEvent::Revision(RevisionEvent::new(id)));
                } else {
                    return None;
                }
            };
        }
    }
}

fn insert<V>(
    data: &mut Index<V>,
    entry: EntryIndex,
    attr: AttributeIndex,
    values: &EntryValues,
) -> bool
where
    V: IndexableValue,
    for<'a> &'a EntryValue: TryInto<V>,
    EntryValue: From<V>,
{
    // first remove the attriebute
    let mut removed = BTreeSet::new();
    data.iter_with_mut(|(value, attrs)| {
        attrs.with_mut(&attr, |entries| {
            let indices = entries.remove(&entry);
            let some = indices.is_some();
            removed.extend(
                indices
                    .into_iter()
                    .flatten()
                    .map(|idx| (idx.0, value.clone())),
            );
            some
        })
    });

    // then insert
    let mut inserted = BTreeSet::new();
    for (index, value) in values.iter().enumerate() {
        let Some(v): Option<V> = value.as_ref().and_then(|v| v.try_into().ok()) else {
            continue;
        };

        let attributes = data.entry(v.clone()).or_default();
        let collections = attributes.entry(attr).or_default();
        let document = collections.entry(entry).or_default();
        if document.insert(index.into()) {
            inserted.insert((index, v));
        }
    }

    // the index is modified if there are any diffs between removed and inserted
    inserted.symmetric_difference(&removed).next().is_some()
}

fn remove<V>(data: &mut Index<V>, entry: EntryIndex) -> Vec<IndexStoreEvent>
where
    V: IndexableValue,
    EntryValue: From<V>,
{
    let mut removed: HashMap<_, BTreeMap<_, _>> = HashMap::new();
    data.iter_with_mut(|(value, attrs)| {
        attrs.iter_with_mut(|(attr_index, value_posting)| {
            if let Some(ids) = value_posting.remove(&entry) {
                let values = removed.entry((entry, *attr_index)).or_default();
                values.extend(ids.into_iter().map(|id| (id, value.clone())));
                true
            } else {
                false
            }
        })
    });
    removed
        .into_iter()
        .map(|((entry, attr), value)| IndexStoreEvent::Removed {
            entry,
            attr,
            value: value
                .into_values()
                .map(EntryValue::from)
                .map(Some)
                .collect::<Vec<_>>(),
        })
        .collect()
}
