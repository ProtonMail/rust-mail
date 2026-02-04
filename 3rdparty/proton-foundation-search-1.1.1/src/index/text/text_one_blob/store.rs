use std::collections::VecDeque;
use std::ops::ControlFlow;

use super::*;
use crate::blob::{Blob, BlobId, RevisionEvent};
use crate::index::prelude::*;

impl IndexStore for TextIndexSansIo {
    fn id(&self) -> &str {
        "text"
    }

    fn write(
        &self,
        revision: Option<BlobId>,
        operations: &[IndexStoreOperation],
    ) -> Box<dyn Send + Iterator<Item = IndexStoreEvent>> {
        Box::new(Writer::new(self, revision, operations))
    }
}

#[derive(Debug)]
struct Writer {
    blob: Option<Blob<TextIndex>>,
    index: Option<TextIndex>,
    operations: VecDeque<IndexStoreOperation>,
    events: VecDeque<IndexStoreEvent>,
    modified: bool,
}

impl Writer {
    fn new(
        _index: &TextIndexSansIo,
        revision: Option<BlobId>,
        operations: &[IndexStoreOperation],
    ) -> Self {
        Self {
            blob: revision.map(|revision| Blob::new(revision, NAME)),
            index: None,
            operations: operations.iter().cloned().collect(),
            events: [].into(),
            modified: false,
        }
    }
}

impl Iterator for Writer {
    type Item = IndexStoreEvent;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(event) = self.events.pop_front() {
                return Some(event);
            }

            let index = match &mut self.index {
                Some(index) => index,
                loading @ None => match self.blob.as_mut().map(Blob::fetch_and_free) {
                    Some(ControlFlow::Break(load)) => return Some(IndexStoreEvent::Load(load)),
                    Some(ControlFlow::Continue(loaded)) => loading.insert(loaded.as_ref().clone()),
                    None => loading.insert(Default::default()),
                },
            };

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
                    if std::mem::take(&mut self.modified) {
                        let index = std::mem::take(index);
                        let id = BlobId::generate(&index);
                        let (release, save) = self
                            .blob
                            .get_or_insert_with(|| Blob::new(id, NAME))
                            .save_and_free(id, Arc::new(index));
                        self.events.push_back(IndexStoreEvent::Save(save));
                        self.events.extend(release.map(IndexStoreEvent::Release));
                        self.events
                            .push_back(IndexStoreEvent::Revision(RevisionEvent::new(id)));
                        continue;
                    } else {
                        None
                    }
                }
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use test_log::test;

    use super::*;
    use crate::serialization::SerDes;

    #[test]
    fn insert() {
        let mut blobs = BTreeMap::new();
        let sut = TextIndexSansIo::default();
        let mut revision = None;

        for round in 0..2u32 {
            // act - insert some content
            // do it twice so we test blob release as well
            let write = sut.write(
                revision,
                &[IndexStoreOperation::Insert(
                    round.into(),
                    0.into(),
                    Arc::new(vec![Some(vec![(123, "howdy".into())].into())]),
                )],
            );
            for event in write {
                match event {
                    IndexStoreEvent::Save(save) => {
                        blobs.insert(
                            save.id(),
                            String::from_utf8(
                                save.recv().serialize(&SerDes::JsonPretty).expect("recv"),
                            )
                            .expect("json"),
                        );
                    }
                    IndexStoreEvent::Load(load) => {
                        if let Some(data) = blobs.get(&load.id()) {
                            load.send(&SerDes::Json, data.as_bytes()).expect("send");
                        } else {
                            load.send_empty().expect("send");
                        }
                    }
                    IndexStoreEvent::Release(release_event) => {
                        blobs
                            .entry(release_event.id())
                            .and_modify(|data| *data = ["released: ", data.as_str()].concat())
                            .or_insert("released.".to_owned());
                    }
                    IndexStoreEvent::Inserted { .. } | IndexStoreEvent::Removed { .. } => {}
                    IndexStoreEvent::Revision(revision_event) => {
                        revision = Some(revision_event.id())
                    }
                }
            }
        }

        // assert blobs - one released one up to date
        let blobs = blobs
            .into_iter()
            .map(|(k, v)| format!("{k}: {v}\n"))
            .collect::<String>();
        insta::assert_snapshot!(blobs, @r#"
        413BB0F93DF45B56B917A35949E99EEE: {
          "occurrences": [
            [
              0,
              0
            ],
            [
              1,
              0
            ]
          ],
          "tokens": [
            [
              "howdy",
              {
                "0": {
                  "0": [
                    123
                  ]
                },
                "1": {
                  "0": [
                    123
                  ]
                }
              }
            ]
          ],
          "trigrams": {
            "how": {
              "0": [
                0
              ]
            },
            "owd": {
              "1": [
                0
              ]
            },
            "wdy": {
              "2": [
                0
              ]
            }
          },
          "stats": {
            "sizes": {
              "0": {
                "0": [
                  5,
                  1
                ],
                "1": [
                  5,
                  1
                ]
              }
            }
          }
        }
        4E10C0E564605361A6E0A376D0838E0C: released: {
          "occurrences": [
            [
              0,
              0
            ]
          ],
          "tokens": [
            [
              "howdy",
              {
                "0": {
                  "0": [
                    123
                  ]
                }
              }
            ]
          ],
          "trigrams": {
            "how": {
              "0": [
                0
              ]
            },
            "owd": {
              "1": [
                0
              ]
            },
            "wdy": {
              "2": [
                0
              ]
            }
          },
          "stats": {
            "sizes": {
              "0": {
                "0": [
                  5,
                  1
                ]
              }
            }
          }
        }
        "#);
    }

    #[test]
    fn insert_rev0() {
        let sut = TextIndexSansIo::default();
        let mut write = sut.write(
            None,
            &[IndexStoreOperation::Insert(
                1.into(),
                2.into(),
                Arc::new(vec![Some(vec![(123, "howdy".into())].into())]),
            )],
        );

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
            Some(IndexStoreEvent::Save(save))
                if save.id().to_string() == "F0D8A251D3F15ACA95C953785CD549E8" =>
            {
                insta::assert_debug_snapshot!(save.recv().serialize(&SerDes::Json).map(String::from_utf8), @r#"
                Ok(
                    Ok(
                        "{\"occurrences\":[[1,2]],\"tokens\":[[\"howdy\",{\"0\":{\"0\":[123]}}]],\"trigrams\":{\"how\":{\"0\":[0]},\"owd\":{\"1\":[0]},\"wdy\":{\"2\":[0]}},\"stats\":{\"sizes\":{\"2\":{\"1\":[5,1]}}}}",
                    ),
                )
                "#);
            }
            next => panic!("unexpected {next:?}"),
        }

        match write.next() {
            Some(IndexStoreEvent::Revision(revision_event))
                if revision_event.id().to_string() == "F0D8A251D3F15ACA95C953785CD549E8" => {}
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

        let mut write = sut.write(
            Some(BlobId::new(0, 4)),
            &[IndexStoreOperation::Remove(1.into())],
        );

        match write.next() {
            Some(IndexStoreEvent::Load(load)) if load.id() == BlobId::new(0, 4) => {
                load.send(&SerDes::Json, b"{\"occurrences\":[[1,2]],\"tokens\":[[\"howdy\",{\"0\":{\"0\":[123]}}]],\"trigrams\":{\"how\":{\"0\":[0]},\"owd\":{\"1\":[0]},\"wdy\":{\"2\":[0]}},\"stats\":{\"sizes\":{\"2\":{\"1\":[5,1]}}}}")
                    .expect("send");
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
                        Some(
                            Text(
                                [
                                    (
                                        123,
                                        "howdy",
                                    ),
                                ],
                            ),
                        ),
                    ],
                }
                "#);
            }
            next => panic!("unexpected {next:?}"),
        }

        match write.next() {
            Some(IndexStoreEvent::Save(save))
                if save.id().to_string() == "38862005911856F08FB635D6937755D6" =>
            {
                let blob = save.recv().serialize(&SerDes::Json).map(String::from_utf8);
                insta::assert_debug_snapshot!(blob, @r#"
                Ok(
                    Ok(
                        "{\"occurrences\":[],\"tokens\":[],\"trigrams\":{},\"stats\":{\"sizes\":{}}}",
                    ),
                )
                "#);
            }
            next => panic!("unexpected {next:?}"),
        }

        match write.next() {
            Some(IndexStoreEvent::Release(release_event))
                if release_event.id() == BlobId::new(0, 4) => {}
            next => panic!("unexpected {next:?}"),
        }

        match write.next() {
            Some(IndexStoreEvent::Revision(revision_event))
                if revision_event.id().to_string() == "38862005911856F08FB635D6937755D6" => {}
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

        let mut write = sut.write(
            Some(BlobId::new(0, 4)),
            &[IndexStoreOperation::Remove(100000000.into())],
        );

        match write.next() {
            Some(IndexStoreEvent::Load(load)) if load.id() == BlobId::new(0, 4) => {
                load.send(&SerDes::Json, b"{\"occurrences\":[[1,2]],\"tokens\":[[\"howdy\",{\"0\":{\"0\":[123]}}]],\"trigrams\":{\"how\":{\"0\":[0]},\"owd\":{\"1\":[0]},\"wdy\":{\"2\":[0]}},\"stats\":{\"sizes\":{\"2\":{\"1\":[5,1]}}}}")
                    .expect("send");
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
