use std::collections::BTreeMap;

use serde::Deserialize;

use crate::entry::EntryValue;
use crate::index::prelude::{AttributeIndex, EntryIndex, IndexExport, IndexExportEvent};
use crate::index::trivial::{Index, IndexableValue, Trivial};
use crate::transaction::{Read, TransactionState};

impl<V> IndexExport for Trivial<V>
where
    V: IndexableValue,
    V: for<'de> Deserialize<'de>,
    V: Default,
    EntryValue: From<V>,
{
    fn export(
        &self,
        revision: u64,
    ) -> Box<dyn 'static + Send + Iterator<Item = crate::index::prelude::IndexExportEvent>> {
        Box::new(Export::new(revision, self))
    }
}

#[derive(Default)]
enum Export<V: IndexableValue> {
    Loading {
        state: TransactionState<Read<Index<V>>, Index<V>>,
    },
    Iterating {
        results: Box<dyn 'static + Send + Iterator<Item = IndexExportEvent>>,
    },
    #[default]
    Done,
}

impl<V> Export<V>
where
    V: IndexableValue,
    V: for<'de> Deserialize<'de>,
    V: Default,
{
    fn new(revision: u64, index: &Trivial<V>) -> Self {
        Self::Loading {
            state: TransactionState::read(
                revision,
                Trivial::<V>::name().into(),
                index.writer.load_full(),
                index.reader.clone(),
            ),
        }
    }
}

impl<V> Iterator for Export<V>
where
    V: for<'de> Deserialize<'de>,
    V: IndexableValue,
    V: Default,
    EntryValue: From<V>,
{
    type Item = IndexExportEvent;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            break match std::mem::take(self) {
                Export::Done => None,
                Export::Iterating { mut results } => {
                    let next = results.next();
                    *self = Export::Iterating { results };
                    next
                }
                Export::Loading { mut state } => {
                    match state.load()? {
                        Ok(index) => {
                            // we have loaded and will just iterate now
                            *self = Self::Iterating {
                                results: Box::new(index
                                    .iter()
                                    .flat_map(|(v, attrs)| {
                                        attrs.iter().map(move|(a, entries)| (entries, *a, v))
                                    })
                                    .flat_map(|(entries, a, v)| {
                                        entries.iter().map(move|(e, positions)| (*e, a, v, positions))
                                    })
                                    .fold(BTreeMap::new(), |mut map: BTreeMap<(EntryIndex, AttributeIndex), Vec<EntryValue>>, (e, a, v, positions)| {
                                        let values = map.entry((e, a)).or_default();

                                        let count = values.len().max( 1 + positions.last().map(|v|v.0).unwrap_or_default());
                                        values.resize(count, EntryValue::from(V::default()));

                                        for position in positions {
                                            values[position.0] = EntryValue::from(v.clone());
                                        }

                                        map
                                    })
                                    .into_iter()
                                    .map(|((e, a), v)| IndexExportEvent::Entry {
                                        entry: e,
                                        attr: a,
                                        value:v,
                                    })),
                            };
                            continue;
                        }
                        Err(load) => {
                            // still loading, preserve self as is
                            *self = Self::Loading { state };
                            Some(IndexExportEvent::Load(load))
                        }
                    }
                }
            };
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use crate::index::prelude::IndexExport;
    use crate::index::trivial::Trivial;

    #[test]
    fn impl_export() {
        fn test(_index: &dyn IndexExport) {}
        test(&Trivial::<bool>::default());
        test(&Trivial::<u64>::default());
        test(&Trivial::<Box<str>>::default());
    }

    #[test]
    fn exports() {
        use std::sync::Arc;

        use crate::index::prelude::*;

        let sut = Trivial::<u64>::default();

        let write = sut.write(
            0,
            &[
                IndexStoreOperation::Insert(1.into(), 0.into(), Arc::new(vec![5.into()])),
                IndexStoreOperation::Insert(
                    0.into(),
                    0.into(),
                    Arc::new([1, 2, 3].into_iter().map(Into::into).collect::<Vec<_>>()),
                ),
                IndexStoreOperation::Insert(
                    0.into(),
                    1.into(),
                    Arc::new([3, 4].into_iter().map(Into::into).collect::<Vec<_>>()),
                ),
            ],
        );

        for event in write {
            if let crate::index::prelude::IndexStoreEvent::Load(load_event) = event {
                load_event.send_empty().expect("send")
            }
        }

        let export = sut.export(1).collect::<Vec<_>>();

        // note that the output is sorted by entry-atribute
        insta::assert_debug_snapshot!(export, @r"
        [
            Entry {
                entry: EntryIndex(
                    0,
                ),
                attr: AttributeIndex(
                    0,
                ),
                value: [
                    Integer(
                        1,
                    ),
                    Integer(
                        2,
                    ),
                    Integer(
                        3,
                    ),
                ],
            },
            Entry {
                entry: EntryIndex(
                    0,
                ),
                attr: AttributeIndex(
                    1,
                ),
                value: [
                    Integer(
                        3,
                    ),
                    Integer(
                        4,
                    ),
                ],
            },
            Entry {
                entry: EntryIndex(
                    1,
                ),
                attr: AttributeIndex(
                    0,
                ),
                value: [
                    Integer(
                        5,
                    ),
                ],
            },
        ]
        ");
    }
}
