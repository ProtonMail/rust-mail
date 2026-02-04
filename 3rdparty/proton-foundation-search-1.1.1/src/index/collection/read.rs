use std::ops::ControlFlow;

use super::*;
use crate::blob::{Blob, BlobId, LoadEvent};

impl CollectionSansIo {
    pub fn read(&self, revision: BlobId) -> Read {
        Read::new(self, revision)
    }
}

pub enum CollectionReadEvent {
    Load(LoadEvent),
    Ready(Arc<CollectionContent>),
}

pub struct Read {
    blob: Option<Blob<CollectionContent>>,
}

impl Read {
    pub fn new(_collection: &CollectionSansIo, revision: BlobId) -> Self {
        Self {
            blob: Some(Blob::<CollectionContent>::new(revision, COLLECTION)),
        }
    }
}

impl Iterator for Read {
    type Item = CollectionReadEvent;

    fn next(&mut self) -> Option<Self::Item> {
        let ready = match self.blob.as_mut()?.fetch_and_free() {
            ControlFlow::Continue(cached) => cached,
            ControlFlow::Break(load) => return Some(CollectionReadEvent::Load(load)),
        };
        // we won't do it again
        self.blob = None;
        Some(CollectionReadEvent::Ready(ready))
    }
}

#[cfg(test)]
mod tests {

    use std::collections::BTreeMap;

    use super::*;
    use crate::serialization::SerDes;

    #[test]
    fn read() {
        let mut blobs = BTreeMap::new();
        let sut = CollectionSansIo::default();

        // arrange some content
        let write = sut.write(
            None,
            &[CollectionWriteOperation::Insert(
                "abc".into(),
                [("x".into(), Arc::new(vec![Some(1.into())]))].into(),
            )],
        );
        let mut revision = None;
        for event in write {
            match event {
                CollectionStoreEvent::Release(release) => {
                    blobs.remove(&release.id());
                }
                CollectionStoreEvent::Load(load_event) => {
                    load_event.send_empty().expect("send");
                }
                CollectionStoreEvent::Save(save) => {
                    let id = save.id();
                    revision = Some(id);
                    let cached = save.recv();
                    let data = cached.serialize(&SerDes::Json).expect("recv");
                    blobs.insert(id, (data, cached));
                }
                _ => {}
            }
        }

        // check that we have some blobs
        let dump = blobs
            .iter()
            .map(|(k, (data, _cached))| format!("{k}: {}\n", str::from_utf8(data).expect("utf-8")))
            .collect::<String>();
        insta::assert_snapshot!(dump, @r#"7CFD3D7456F0520DB328C00118BD26EB: {"attributes":["x"],"entries":{"abc":0},"identifiers":{"0":"abc"}}"#);

        let read = sut.read(revision.expect("revision"));
        for event in read {
            match event {
                CollectionReadEvent::Load(load) => {
                    if let Some((_data, cached)) = blobs.get(&load.id()) {
                        load.send_cached(cached).expect("cached send");
                    } else {
                        unreachable!("must be cached");
                    }
                }
                CollectionReadEvent::Ready(cached) => {
                    insta::assert_debug_snapshot!(cached, @r#"
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
                    }
                    "#);
                }
            }
        }
    }
}
