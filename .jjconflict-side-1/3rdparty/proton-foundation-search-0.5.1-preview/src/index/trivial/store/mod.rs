use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

use super::*;
use crate::entry::EntryValue;
use crate::index::extensions::WithMut;
use crate::index::prelude::{IndexStore, IndexStoreEvent, IndexStoreOperation};
use crate::transaction::{TransactionState, Write};

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
        revision: u64,
        operations: &[IndexStoreOperation],
    ) -> Box<dyn Send + Iterator<Item = IndexStoreEvent>> {
        let write = Writer::new(self, revision, operations);
        Box::new(write)
    }
    fn reset(&self) {
        self.reader.store(None);
        self.writer.store(None);
    }
}

struct Writer<V>
where
    V: IndexableValue,
{
    operations: VecDeque<IndexStoreOperation>,
    iterating: VecDeque<IndexStoreEvent>,
    modified: bool,
    state: Option<TransactionState<Write<Index<V>>, Index<V>>>,
}

impl<V> Writer<V>
where
    V: IndexableValue + for<'de> Deserialize<'de>,
{
    fn new(index: &Trivial<V>, revision: u64, operations: &[IndexStoreOperation]) -> Self {
        // TODO: we could batch multiple subsequent removals or inserts here to avoid multiple loops, keeping it simple now
        let operations = operations.iter().cloned().collect();

        let state = Some(TransactionState::write(
            revision,
            Trivial::<V>::name().into(),
            index.reader.load_full(),
            index.writer.clone(),
        ));

        Self {
            operations,
            iterating: [].into(),
            modified: false,
            state,
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

            let index = match self.state.as_mut()?.load()? {
                Ok(cached) => cached,
                Err(load) => return Some(IndexStoreEvent::Load(load)),
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
                if let (true, Some(state)) = (
                    std::mem::take(&mut self.modified),
                    std::mem::take(&mut self.state),
                ) {
                    let (save, release) = state.save();
                    self.iterating.push_back(IndexStoreEvent::Save(save));
                    if let Some(release) = release {
                        self.iterating.push_back(IndexStoreEvent::Release(release));
                    }
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
        let Some(v) = value.try_into().ok() else {
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
                .collect::<Vec<_>>(),
        })
        .collect()
}
