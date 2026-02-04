use std::collections::BTreeMap;
use std::ops::ControlFlow;

use serde::Deserialize;

use crate::blob::{Blob, BlobId};
use crate::entry::EntryValue;
use crate::index::prelude::{AttributeIndex, EntryIndex, IndexExport, IndexExportEvent};
use crate::index::trivial::{Index, IndexableValue, Trivial};

impl<V> IndexExport for Trivial<V>
where
    V: IndexableValue,
    V: for<'de> Deserialize<'de>,
    V: Default,
    EntryValue: From<V>,
{
    fn export(
        &self,
        revision: BlobId,
    ) -> Box<dyn 'static + Send + Iterator<Item = crate::index::prelude::IndexExportEvent>> {
        Box::new(Export::new(self, revision))
    }
}

#[derive(Default)]
enum Export<V: IndexableValue> {
    Loading {
        blob: Blob<Index<V>>,
    },
    Iterating {
        results: Box<dyn 'static + Send + Iterator<Item = IndexExportEvent>>,
    },
    #[default]
    Done,
}

impl<V: IndexableValue> Export<V> {
    fn new(_index: &Trivial<V>, revision: BlobId) -> Self {
        Self::Loading {
            blob: Blob::new(revision, Trivial::<V>::name()),
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
                Export::Loading { mut blob } => {
                    match blob.fetch_and_free() {
                        ControlFlow::Continue(index) => {
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
                                    .fold(BTreeMap::new(), |mut map: BTreeMap<(EntryIndex, AttributeIndex), Vec<Option<EntryValue>>>, (e, a, v, positions)| {
                                        let values = map.entry((e, a)).or_default();

                                        let count = values.len().max( 1 + positions.last().map(|v|v.0).unwrap_or_default());
                                        values.resize(count, None);

                                        for position in positions {
                                            values[position.0] = Some(EntryValue::from(v.clone()));
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
                        ControlFlow::Break(load) => {
                            // still loading, preserve self as is
                            *self = Self::Loading { blob };
                            Some(IndexExportEvent::Load(load))
                        }
                    }
                }
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

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

        let mut revision = None;
        let mut cache = BTreeMap::new();
        let sut = Trivial::<u64>::default();

        let write = sut.write(
            revision,
            &[
                IndexStoreOperation::Insert(1.into(), 0.into(), Arc::new(vec![Some(5.into())])),
                IndexStoreOperation::Insert(
                    0.into(),
                    0.into(),
                    Arc::new(
                        [1, 2, 3]
                            .into_iter()
                            .map(Into::into)
                            .map(Some)
                            .collect::<Vec<_>>(),
                    ),
                ),
                IndexStoreOperation::Insert(
                    0.into(),
                    1.into(),
                    Arc::new(
                        [3, 4]
                            .into_iter()
                            .map(Into::into)
                            .map(Some)
                            .collect::<Vec<_>>(),
                    ),
                ),
            ],
        );

        for event in write {
            match event {
                IndexStoreEvent::Inserted { .. } | IndexStoreEvent::Removed { .. } => {}
                IndexStoreEvent::Load(load_event) => {
                    load_event.send_empty().expect("send");
                }
                IndexStoreEvent::Save(save_event) => {
                    cache.insert(save_event.id(), save_event.recv());
                }
                IndexStoreEvent::Release(release_event) => {
                    cache.remove(&release_event.id());
                }
                IndexStoreEvent::Revision(revision_event) => {
                    revision = Some(revision_event.id());
                }
            }
        }

        let export = sut
            .export(revision.expect("revision"))
            .filter_map(|event| match event {
                IndexExportEvent::Load(load_event) => {
                    let cached = cache.get(&load_event.id()).expect("must be cached");
                    load_event.send_cached(cached).expect("send cached");
                    None
                }
                IndexExportEvent::Entry { entry, attr, value } => Some((entry, attr, value)),
            })
            .collect::<Vec<_>>();

        // note that the output is sorted by entry-atribute
        insta::assert_debug_snapshot!(export, @r"
        [
            (
                EntryIndex(
                    0,
                ),
                AttributeIndex(
                    0,
                ),
                [
                    Some(
                        Integer(
                            1,
                        ),
                    ),
                    Some(
                        Integer(
                            2,
                        ),
                    ),
                    Some(
                        Integer(
                            3,
                        ),
                    ),
                ],
            ),
            (
                EntryIndex(
                    0,
                ),
                AttributeIndex(
                    1,
                ),
                [
                    Some(
                        Integer(
                            3,
                        ),
                    ),
                    Some(
                        Integer(
                            4,
                        ),
                    ),
                ],
            ),
            (
                EntryIndex(
                    1,
                ),
                AttributeIndex(
                    0,
                ),
                [
                    Some(
                        Integer(
                            5,
                        ),
                    ),
                ],
            ),
        ]
        ");
    }
}
