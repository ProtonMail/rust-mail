use super::*;

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
impl Engine {
    /// Cleanup the engine's released blobs.
    /// This is intentionally a separate write transaction to avoid race conditions with readers trying to load removed blobs.
    pub fn reset(&self) -> Option<Cleanup> {
        Cleanup::start(self, true)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use test_log::test;

    use crate::blob::{BlobId, Cached, RevisionEvent, SaveEvent};
    use crate::document::{Document, Value};
    use crate::engine::{Engine, Stats};
    use crate::index::prelude::{IndexExport, IndexSearch, IndexStore, IndexStoreEvent};
    use crate::serialization::SerDes;

    #[test]
    fn reset_implements_send() {
        fn sendy(_send: impl Send) {}
        sendy(Engine::builder().build().reset())
    }

    #[test]
    fn reset() {
        let sut = Engine::builder()
            .with_builtin_processor(&Default::default())
            .with_index(FakeIndex {
                name: "a",
                blobs: vec![BlobId::new(1, 0), BlobId::new(1, 1), BlobId::new(1, 2)],
            })
            .with_index(FakeIndex {
                name: "b",
                blobs: vec![BlobId::new(2, 0), BlobId::new(2, 1), BlobId::new(2, 2)],
            })
            .expect("idx")
            .with_index(FakeIndex {
                name: "c",
                blobs: vec![BlobId::new(3, 0), BlobId::new(3, 1), BlobId::new(3, 2)],
            })
            .expect("idx")
            .build();
        let mut blobs = BTreeMap::default();
        let mut stats = None;

        // arrange some content
        let mut write = sut.write().expect("write");
        write.insert(
            Document::new("abc")
                .with_attribute("i", 1)
                .with_attribute("t", Value::text("abc, xyz")),
        );
        for event in write.commit() {
            match event {
                crate::engine::WriteEvent::Load(load_event) => {
                    load_event.send_empty().expect("send");
                }
                crate::engine::WriteEvent::Save(save_event) => {
                    blobs.insert(
                        save_event.id(),
                        save_event
                            .recv()
                            .serialize(&SerDes::JsonPretty)
                            .expect("recv"),
                    );
                }
                crate::engine::WriteEvent::Stats(s) => stats = Some(s),
            }
        }

        assert_eq!(stats, Some(Stats { documents: 1 }));

        // check that the engine has content
        let dump = blobs
            .iter()
            .map(|(k, v)| format!("{k}: {}\n", str::from_utf8(v).expect("json")))
            .collect::<String>();
        insta::assert_snapshot!(dump, @r#"
        00000000000000000000000000000000: {
          "collection_revision": "1A7986D4C2265C43AF273EC46B66516D",
          "index_revisions": {
            "a": "00000000000000010000000000000002",
            "b": "00000000000000020000000000000002",
            "c": "00000000000000030000000000000002"
          },
          "active_blobs": [
            "00000000000000010000000000000000",
            "00000000000000010000000000000001",
            "00000000000000010000000000000002",
            "00000000000000020000000000000000",
            "00000000000000020000000000000001",
            "00000000000000020000000000000002",
            "00000000000000030000000000000000",
            "00000000000000030000000000000001",
            "00000000000000030000000000000002",
            "1A7986D4C2265C43AF273EC46B66516D"
          ],
          "released_blobs": []
        }
        00000000000000010000000000000000: "a 00000000000000010000000000000000"
        00000000000000010000000000000001: "a 00000000000000010000000000000001"
        00000000000000010000000000000002: "a 00000000000000010000000000000002"
        00000000000000020000000000000000: "b 00000000000000020000000000000000"
        00000000000000020000000000000001: "b 00000000000000020000000000000001"
        00000000000000020000000000000002: "b 00000000000000020000000000000002"
        00000000000000030000000000000000: "c 00000000000000030000000000000000"
        00000000000000030000000000000001: "c 00000000000000030000000000000001"
        00000000000000030000000000000002: "c 00000000000000030000000000000002"
        1A7986D4C2265C43AF273EC46B66516D: {
          "attributes": [
            "i",
            "t"
          ],
          "entries": {
            "abc": 0
          },
          "identifiers": {
            "0": "abc"
          }
        }
        "#);

        // act - reset
        for event in sut.reset().expect("reset") {
            match event {
                crate::engine::CleanupEvent::Release(release_event) => {
                    blobs.remove(&release_event.id());
                }
                crate::engine::CleanupEvent::Load(load_event) => {
                    let blob = blobs
                        .get(&load_event.id())
                        .unwrap_or_else(|| panic!("blob {}", load_event.id()));
                    load_event
                        .send(&SerDes::Json, blob.as_slice())
                        .expect("send");
                }
                crate::engine::CleanupEvent::Save(..) => {
                    unreachable!("reset won't save anything")
                }
            }
        }

        // assert that we have removed all blobs
        assert!(blobs.is_empty());
    }
    #[derive(Debug)]
    struct FakeIndex {
        name: &'static str,
        blobs: Vec<BlobId>,
    }
    impl IndexStore for FakeIndex {
        fn id(&self) -> &str {
            self.name
        }

        fn write(
            &self,
            _revision: Option<BlobId>,
            _operations: &[crate::index::prelude::IndexStoreOperation],
        ) -> Box<dyn Send + Iterator<Item = crate::index::prelude::IndexStoreEvent>> {
            let name = self.name;
            Box::new(
                self.blobs
                    .clone()
                    .into_iter()
                    .map(move |id| {
                        IndexStoreEvent::Save(SaveEvent::new(
                            id,
                            Cached::new(name, Arc::new(format!("{name} {id}"))),
                        ))
                    })
                    .chain(
                        self.blobs
                            .last()
                            .copied()
                            .map(|id| IndexStoreEvent::Revision(RevisionEvent::new(id))),
                    ),
            )
        }
    }
    impl IndexExport for FakeIndex {
        fn export(
            &self,
            _revision: BlobId,
        ) -> Box<dyn 'static + Send + Iterator<Item = crate::index::prelude::IndexExportEvent>>
        {
            unreachable!()
        }
    }
    impl IndexSearch for FakeIndex {
        fn search(
            &self,
            _revision: BlobId,
            _attribute: Option<crate::index::prelude::AttributeIndex>,
            _function: crate::query::expression::Func,
            _value: &crate::query::expression::TermValue,
            _options: &crate::query::option::QueryOptions,
        ) -> Option<
            Box<dyn 'static + Send + Iterator<Item = crate::index::prelude::IndexSearchEvent>>,
        > {
            unreachable!()
        }
    }
}
