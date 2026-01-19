use super::*;
use crate::transaction::{Cached, LoadEvent, StaticRead, TransactionState};

impl CollectionSansIo {
    pub fn read(&self, revision: u64) -> Read {
        Read::new(self, revision)
    }
}

pub enum CollectionReadEvent {
    Load(LoadEvent),
    Ready(Cached<CollectionContent>),
}

pub struct Read {
    state: Option<TransactionState<StaticRead<CollectionContent>, CollectionContent>>,
}

impl Read {
    pub fn new(collection: &CollectionSansIo, revision: u64) -> Self {
        Self {
            state: Some(TransactionState::static_read(
                revision,
                COLLECTION.into(),
                collection.writer.load_full(),
                collection.reader.clone(),
            )),
        }
    }
}

impl Iterator for Read {
    type Item = CollectionReadEvent;

    fn next(&mut self) -> Option<Self::Item> {
        let ready = match self.state.as_mut()?.load()? {
            Ok(cached) => cached,
            Err(load) => return Some(CollectionReadEvent::Load(load)),
        };
        // we won't do it again
        self.state = None;
        Some(CollectionReadEvent::Ready(ready))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::collections::BTreeMap;

    use super::*;
    use crate::serialization::SerDes;
    use crate::transaction::{ReleaseEvent, SaveEvent};

    #[test]
    fn read() {
        let mut blobs = BTreeMap::new();
        let sut = CollectionSansIo::default();

        // arrange some content
        let write = sut.write(
            0,
            &[CollectionWriteOperation::Insert(
                "abc".into(),
                [("x".into(), Arc::new(vec![1.into()]))].into(),
            )],
            0,
        );
        for event in write {
            match event {
                CollectionStoreEvent::Release(ReleaseEvent { name }) => {
                    blobs.remove(&name);
                }
                CollectionStoreEvent::Load(LoadEvent { send, .. }) => {
                    (send)(&crate::serialization::SerDes::Cbor, vec![]).expect("send")
                }
                CollectionStoreEvent::Save(SaveEvent { name, recv }) => {
                    blobs.insert(name, (recv)(&SerDes::Json).expect("recv"));
                }
                _ => {}
            }
        }

        // check that we have some blobs
        insta::assert_debug_snapshot!(blobs.into_iter().map(|(k,v)|(k,String::from_utf8(v).expect("utf-8"))).collect::<Vec<_>>(), @r#"
        [
            (
                "collection r1",
                "[1,{\"attributes\":[\"x\"],\"entries\":{\"abc\":0},\"identifiers\":{\"0\":\"abc\"}}]",
            ),
        ]
        "#);
        // check that the collection has content
        insta::assert_debug_snapshot!(sut, @r#"
        CollectionSansIo {
            reader: ArcSwapAny(
                None,
            ),
            writer: ArcSwapAny(
                Some(
                    (
                        1,
                        CollectionContent {
                            attributes: {
                                "x",
                            },
                            entries: {
                                "abc": EntryIndex(
                                    0,
                                ),
                            },
                            identifiers: {
                                EntryIndex(
                                    0,
                                ): "abc",
                            },
                        },
                    ),
                ),
            ),
        }
        "#);

        let read = sut.read(1);
        for event in read {
            match event {
                CollectionReadEvent::Load(..) => {
                    unreachable!("should be cached from previous write")
                }
                CollectionReadEvent::Ready(cached) => {
                    insta::assert_debug_snapshot!(cached, @r#"
                    Cached(
                        (
                            1,
                            CollectionContent {
                                attributes: {
                                    "x",
                                },
                                entries: {
                                    "abc": EntryIndex(
                                        0,
                                    ),
                                },
                                identifiers: {
                                    EntryIndex(
                                        0,
                                    ): "abc",
                                },
                            },
                        ),
                    )
                    "#);
                }
            }
        }
    }
}
