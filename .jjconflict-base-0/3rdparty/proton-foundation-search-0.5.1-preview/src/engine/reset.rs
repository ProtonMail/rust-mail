use std::collections::VecDeque;
use std::sync::atomic::Ordering;

use super::*;
use crate::transaction::TransactionState;

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
impl Engine {
    /// Cleanup the engine's released blobs.
    /// This is intentionally a separate write transaction to avoid race conditions with readers trying to load removed blobs.
    pub fn reset(&self) -> Option<Cleanup> {
        if self.inner.writer.swap(true, Ordering::AcqRel) {
            // already writing
            return None;
        }
        let guard = EngineWriteGuard(self.inner.clone());
        let mut tran: Option<TransactionState<crate::transaction::NoCache<Manifest>, Manifest>> =
            Some(TransactionState::no_cache(
                MANIFEST.into(),
                Manifest::default,
            ));

        // reset caches
        self.inner.collection.reset();
        for index in self.inner.indices.values() {
            index.reset();
        }

        // release all blobs
        let mut resets = VecDeque::new();
        Some(Cleanup(Box::new(std::iter::from_fn(move || {
            // guard will release writer reservation on drop
            let _guard = &guard;

            loop {
                if let Some(reset) = resets.pop_front() {
                    return Some(CleanupEvent::Release(reset));
                }

                let mut state = tran.take()?;
                break match state.load()? {
                    Ok(manifest) => {
                        resets.extend(std::mem::take(&mut manifest.released_blobs));
                        resets.extend(std::mem::take(&mut manifest.active_blobs));
                        resets.push_front(state.reset().name);
                        continue;
                    }
                    Err(load) => {
                        // keep the state for saving later
                        tran = Some(state);
                        Some(CleanupEvent::Load(load))
                    }
                };
            }
        }))))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::collections::BTreeMap;

    use crate::document::{Document, Value};
    use crate::engine::Engine;
    use crate::serialization::SerDes;

    #[test]
    fn reset() {
        let sut = Engine::builder().build();
        let mut blobs = BTreeMap::default();

        // arrange some content
        let mut write = sut.write().expect("write");
        write
            .insert(
                Document::new("abc")
                    .with_attribute("i", 1)
                    .with_attribute("t", Value::text("abc, xyz")),
            )
            .expect("insert");
        for event in write.commit() {
            match event {
                crate::engine::WriteEvent::Modified(_) => {}
                crate::engine::WriteEvent::Load(load_event) => {
                    load_event.send_empty().expect("send");
                }
                crate::engine::WriteEvent::Save(save_event) => {
                    blobs.insert(
                        save_event.name.clone(),
                        (save_event.recv)(&SerDes::Cbor).expect("recv"),
                    );
                }
            }
        }

        // check that we have the blob
        insta::assert_compact_debug_snapshot!(blobs.keys().collect::<Vec<_>>(), @r#"["collection r1", "manifest r0", "text r1", "u64 r1"]"#);
        // check that the engine has content
        insta::assert_debug_snapshot!(sut);

        // act - reset

        for event in sut.reset().expect("reset") {
            match event {
                crate::engine::CleanupEvent::Release(name) => {
                    blobs.remove(&name);
                }
                crate::engine::CleanupEvent::Load(load_event) => {
                    let blob = blobs.get(&load_event.name).expect("blob");
                    (load_event.send)(&SerDes::Cbor, blob.clone()).expect("send");
                }
                crate::engine::CleanupEvent::Save(..) => {
                    unreachable!("reset won't save anything")
                }
            }
        }

        // assert that we have removed all blobs
        assert!(blobs.is_empty());
        // assert that the engine has no content
        insta::assert_debug_snapshot!(sut,@r#"
        Engine {
            inner: InnerEngine {
                collection: CollectionSansIo {
                    reader: ArcSwapAny(
                        None,
                    ),
                    writer: ArcSwapAny(
                        None,
                    ),
                },
                indices: {
                    "bool": Trivial {
                        reader: ArcSwapAny(
                            None,
                        ),
                        writer: ArcSwapAny(
                            None,
                        ),
                    },
                    "tag": Trivial {
                        reader: ArcSwapAny(
                            None,
                        ),
                        writer: ArcSwapAny(
                            None,
                        ),
                    },
                    "text": TextIndexSansIo {
                        reader: ArcSwapAny(
                            None,
                        ),
                        writer: ArcSwapAny(
                            None,
                        ),
                    },
                    "u64": Trivial {
                        reader: ArcSwapAny(
                            None,
                        ),
                        writer: ArcSwapAny(
                            None,
                        ),
                    },
                },
                processor: Processor {
                    text: Processor {
                        min_length: 3,
                        max_length: 20,
                    },
                },
                writer: false,
                current_batch: 0,
            },
        }
        "#);
    }
}
