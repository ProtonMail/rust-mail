use std::collections::VecDeque;

use super::*;
use crate::index::prelude::*;
use crate::transaction::{ReleaseEvent, TransactionState, Write};

impl IndexStore for TextIndexSansIo {
    fn id(&self) -> &str {
        "text"
    }

    fn write(
        &self,
        revision: u64,
        operations: &[IndexStoreOperation],
    ) -> Box<dyn Send + Iterator<Item = IndexStoreEvent>> {
        Box::new(Writer::new(self, revision, operations))
    }

    fn reset(&self) {
        self.reader.store(None);
        self.writer.store(None);
    }
}

#[derive(Debug)]
struct Writer {
    state: Option<TransactionState<Write<TextIndex>, TextIndex>>,
    operations: VecDeque<IndexStoreOperation>,
    events: VecDeque<IndexStoreEvent>,
    modified: bool,
    /// obsolete blob names to release
    released_blobs: VecDeque<ReleaseEvent>,
}

impl Writer {
    fn new(cache: &TextIndexSansIo, revision: u64, operations: &[IndexStoreOperation]) -> Self {
        let state = Some(TransactionState::write(
            revision,
            NAME.into(),
            cache.reader.load_full(),
            cache.writer.clone(),
        ));
        let operations = operations.iter().cloned().collect();
        Self {
            state,
            operations,
            events: [].into(),
            modified: false,
            released_blobs: [].into(),
        }
    }
}

impl Iterator for Writer {
    type Item = IndexStoreEvent;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(released) = self.released_blobs.pop_front() {
            return Some(IndexStoreEvent::Release(released));
        }

        let index = match self.state.as_mut()?.load()? {
            Ok(cached) => cached,
            Err(load) => return Some(IndexStoreEvent::Load(load)),
        };
        loop {
            if let Some(next) = self.events.pop_front() {
                return Some(next);
            }
            break match self.operations.pop_front() {
                Some(op) => {
                    match op {
                        IndexStoreOperation::Insert(entry, attr, items) => {
                            if index.insert(entry, attr, items.as_ref()) {
                                self.modified = true;
                                self.events
                                    .push_back(IndexStoreEvent::Inserted { entry, attr })
                            }
                        }
                        IndexStoreOperation::Remove(entry_index) => {
                            let removed = index.remove(&[entry_index].into());
                            for (entry, attr, value) in removed {
                                self.modified = true;
                                self.events.push_back(IndexStoreEvent::Removed {
                                    entry,
                                    attr,
                                    value,
                                });
                            }
                        }
                    }
                    continue;
                }
                None => {
                    if let (true, Some(state)) = (
                        std::mem::take(&mut self.modified),
                        std::mem::take(&mut self.state),
                    ) {
                        let (save, release) = state.save();
                        if let Some(release) = release {
                            self.released_blobs.push_back(release);
                        }
                        Some(IndexStoreEvent::Save(save))
                    } else {
                        None
                    }
                }
            };
        }
    }
}

#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use test_log::test;

    use super::*;
    use crate::serialization::SerDes;
    use crate::transaction::{LoadEvent, ReleaseEvent, SaveEvent};

    #[test]
    fn insert() {
        let mut blobs = BTreeMap::new();
        let sut = TextIndexSansIo::default();

        for round in 0..2u32 {
            // act - insert some content
            // do it twice so we test blob release as well
            let write = sut.write(
                round as u64,
                &[IndexStoreOperation::Insert(
                    round.into(),
                    0.into(),
                    Arc::new(vec![vec![(123, "howdy".into())].into()]),
                )],
            );
            for event in write {
                match event {
                    IndexStoreEvent::Release(ReleaseEvent { name }) => {
                        blobs
                            .entry(name)
                            .and_modify(|data| {
                                *data = b"released: ".iter().chain(&*data).copied().collect()
                            })
                            .or_insert(b"released.".into());
                    }
                    IndexStoreEvent::Load(LoadEvent { send, .. }) => {
                        (send)(&crate::serialization::SerDes::Cbor, vec![]).expect("send")
                    }
                    IndexStoreEvent::Save(SaveEvent { name, recv }) => {
                        blobs.insert(name, (recv)(&SerDes::Json).expect("recv"));
                    }
                    _ => {}
                }
            }
        }

        // assert blobs - one released one up to date
        insta::assert_debug_snapshot!(blobs.into_iter().map(|(k,v)|(k,String::from_utf8(v).expect("utf-8"))).collect::<Vec<_>>(), @r#"
        [
            (
                "text r0",
                "released.",
            ),
            (
                "text r1",
                "released: [1,{\"occurrences\":[[0,0]],\"tokens\":[[\"howdy\",{\"0\":{\"0\":[123]}}]],\"trigrams\":{\"how\":{\"0\":[0]},\"owd\":{\"1\":[0]},\"wdy\":{\"2\":[0]}},\"stats\":{\"sizes\":{\"0\":{\"0\":[5,1]}}}}]",
            ),
            (
                "text r2",
                "[2,{\"occurrences\":[[0,0],[1,0]],\"tokens\":[[\"howdy\",{\"0\":{\"0\":[123]},\"1\":{\"0\":[123]}}]],\"trigrams\":{\"how\":{\"0\":[0]},\"owd\":{\"1\":[0]},\"wdy\":{\"2\":[0]}},\"stats\":{\"sizes\":{\"0\":{\"0\":[5,1],\"1\":[5,1]}}}}]",
            ),
        ]
        "#);
        // assert that the collection has content
        insta::assert_debug_snapshot!(sut, @r#"
        TextIndexSansIo {
            reader: ArcSwapAny(
                None,
            ),
            writer: ArcSwapAny(
                Some(
                    (
                        2,
                        TextIndex {
                            occurrences: {
                                (
                                    EntryIndex(
                                        0,
                                    ),
                                    AttributeIndex(
                                        0,
                                    ),
                                ),
                                (
                                    EntryIndex(
                                        1,
                                    ),
                                    AttributeIndex(
                                        0,
                                    ),
                                ),
                            },
                            tokens: [
                                (
                                    "howdy",
                                    {
                                        OccurrenceRef(
                                            0,
                                        ): {
                                            ValueIndex(
                                                0,
                                            ): {
                                                TokenPosition(
                                                    123,
                                                ),
                                            },
                                        },
                                        OccurrenceRef(
                                            1,
                                        ): {
                                            ValueIndex(
                                                0,
                                            ): {
                                                TokenPosition(
                                                    123,
                                                ),
                                            },
                                        },
                                    },
                                ),
                            ],
                            trigrams: {
                                "how": {
                                    TrigramPosition(
                                        0,
                                    ): {
                                        TokenRef(
                                            0,
                                        ),
                                    },
                                },
                                "owd": {
                                    TrigramPosition(
                                        1,
                                    ): {
                                        TokenRef(
                                            0,
                                        ),
                                    },
                                },
                                "wdy": {
                                    TrigramPosition(
                                        2,
                                    ): {
                                        TokenRef(
                                            0,
                                        ),
                                    },
                                },
                            },
                            stats: Stats {
                                sizes: {
                                    AttributeIndex(
                                        0,
                                    ): {
                                        EntryIndex(
                                            0,
                                        ): (
                                            5,
                                            1,
                                        ),
                                        EntryIndex(
                                            1,
                                        ): (
                                            5,
                                            1,
                                        ),
                                    },
                                },
                            },
                        },
                    ),
                ),
            ),
        }
        "#);
    }

    #[test]
    fn insert_rev0() {
        let sut = TextIndexSansIo::default();

        let mut write = sut.write(
            0,
            &[IndexStoreOperation::Insert(
                1.into(),
                2.into(),
                Arc::new(vec![vec![(123, "howdy".into())].into()]),
            )],
        );

        match write.next() {
            Some(IndexStoreEvent::Load(LoadEvent { name, send })) if name.as_ref() == "text r0" => {
                send(&SerDes::Json, vec![]).expect("send")
            }
            next => panic!("unexpected {next:?}"),
        }

        match write.next() {
            Some(inserted @ IndexStoreEvent::Inserted { .. }) => {
                insta::assert_debug_snapshot!(inserted, @r"
                Inserted {
                    entry: EntryIndex(
                        1,
                    ),
                    attr: AttributeIndex(
                        2,
                    ),
                }
                ");
            }
            next => panic!("unexpected {next:?}"),
        }

        match write.next() {
            Some(IndexStoreEvent::Save(SaveEvent { name, recv })) if name.as_ref() == "text r1" => {
                insta::assert_debug_snapshot!(recv(&SerDes::Json).map(String::from_utf8), @r#"
                Ok(
                    Ok(
                        "[1,{\"occurrences\":[[1,2]],\"tokens\":[[\"howdy\",{\"0\":{\"0\":[123]}}]],\"trigrams\":{\"how\":{\"0\":[0]},\"owd\":{\"1\":[0]},\"wdy\":{\"2\":[0]}},\"stats\":{\"sizes\":{\"2\":{\"1\":[5,1]}}}}]",
                    ),
                )
                "#);
            }
            next => panic!("unexpected {next:?}"),
        }

        match write.next() {
            Some(IndexStoreEvent::Release(ReleaseEvent { name })) if name.as_ref() == "text r0" => {
            }
            next => panic!("unexpected {next:?}"),
        }

        match write.next() {
            None => {}
            next => panic!("unexpected {next:?}"),
        }
    }

    #[test]
    fn remove_rev4() {
        let sut = TextIndexSansIo::default();

        let mut write = sut.write(4, &[IndexStoreOperation::Remove(1.into())]);

        match write.next() {
            Some(IndexStoreEvent::Load(LoadEvent { name, send }))
                if name.as_ref() == "text r4" =>
            {
                send(&SerDes::Json, b"[4,{\"occurrences\":[[1,2]],\"tokens\":[[\"howdy\",{\"0\":{\"0\":[123]}}]],\"trigrams\":{\"how\":{\"0\":[0]},\"owd\":{\"1\":[0]},\"wdy\":{\"2\":[0]}},\"stats\":{\"sizes\":{\"2\":{\"1\":[5,1]}}}}]".to_vec())
                    .expect("send")
            }
            next => panic!("unexpected {next:?}"),
        }

        match write.next() {
            Some(removed @ IndexStoreEvent::Removed { .. }) => {
                insta::assert_debug_snapshot!(removed, @r#"
                Removed {
                    entry: EntryIndex(
                        1,
                    ),
                    attr: AttributeIndex(
                        2,
                    ),
                    value: [
                        Text(
                            [
                                (
                                    123,
                                    "howdy",
                                ),
                            ],
                        ),
                    ],
                }
                "#);
            }
            next => panic!("unexpected {next:?}"),
        }

        match write.next() {
            Some(IndexStoreEvent::Save(SaveEvent { name, recv })) if name.as_ref() == "text r5" => {
                insta::assert_debug_snapshot!(recv(&SerDes::Json).map(String::from_utf8), @r#"
                Ok(
                    Ok(
                        "[5,{\"occurrences\":[],\"tokens\":[],\"trigrams\":{},\"stats\":{\"sizes\":{}}}]",
                    ),
                )
                "#);
            }
            next => panic!("unexpected {next:?}"),
        }

        match write.next() {
            Some(IndexStoreEvent::Release(ReleaseEvent { name })) if name.as_ref() == "text r4" => {
            }
            next => panic!("unexpected {next:?}"),
        }

        match write.next() {
            None => {}
            next => panic!("unexpected {next:?}"),
        }
    }

    #[test]
    fn unmodified() {
        let sut = TextIndexSansIo::default();

        let mut write = sut.write(4, &[IndexStoreOperation::Remove(100000000.into())]);

        match write.next() {
            Some(IndexStoreEvent::Load(LoadEvent { name, send }))
                if name.as_ref() == "text r4" =>
            {
                send(&SerDes::Json, b"[4,{\"occurrences\":[[1,2]],\"tokens\":[[\"howdy\",{\"0\":{\"0\":[123]}}]],\"trigrams\":{\"how\":{\"0\":[0]},\"owd\":{\"1\":[0]},\"wdy\":{\"2\":[0]}},\"stats\":{\"sizes\":{\"2\":{\"1\":[5,1]}}}}]".to_vec())
                    .expect("send")
            }
            next => panic!("unexpected {next:?}"),
        }

        // Removing absent entry makes no difference
        match write.next() {
            None => {}
            next => panic!("unexpected {next:?}"),
        }
    }
}
