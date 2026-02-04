use std::collections::VecDeque;
use std::ops::ControlFlow;
use std::sync::atomic::Ordering;

use super::*;
use crate::blob::{Blob, LoadEvent, ReleaseEvent, SaveEvent};

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
impl Engine {
    /// Cleanup the engine's released blobs.
    /// This is intentionally a separate write transaction to avoid race conditions with readers trying to load removed blobs.
    pub fn cleanup(&self) -> Option<Cleanup> {
        Cleanup::start(self, false)
    }
}

/// Iterator of the cleanum transaction events
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
pub struct Cleanup {
    reset: bool,
    blob: Blob<Manifest>,
    events: Option<VecDeque<CleanupEvent>>,
    #[allow(dead_code, reason = "guard will release writer reservation on drop")]
    guard: EngineWriteGuard,
}

impl Cleanup {
    pub(crate) fn start(engine: &Engine, reset: bool) -> Option<Self> {
        if engine.inner.writer.swap(true, Ordering::AcqRel) {
            // already writing
            return None;
        }
        let guard = EngineWriteGuard(engine.inner.clone());
        let blob: Blob<Manifest> = Blob::new(MANIFEST_BLOB_ID, MANIFEST);

        Some(Cleanup {
            reset,
            events: None,
            blob,
            guard,
        })
    }
}

impl Iterator for Cleanup {
    type Item = CleanupEvent;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(events) = &mut self.events {
                return events.pop_front();
            }

            match self.blob.load() {
                ControlFlow::Break(load) => return Some(CleanupEvent::Load(load)),
                ControlFlow::Continue(manifest) => {
                    let mut manifest = manifest.as_ref().clone();
                    let events = self.events.get_or_insert_default();

                    events.extend(
                        std::mem::take(&mut manifest.released_blobs)
                            .into_iter()
                            .map(|id| CleanupEvent::Release(ReleaseEvent::new(id))),
                    );

                    if self.reset {
                        events.extend(
                            std::mem::take(&mut manifest.active_blobs)
                                .into_iter()
                                .map(|id| CleanupEvent::Release(ReleaseEvent::new(id))),
                        );
                        // reset, not saving the manifest
                        events.push_back(CleanupEvent::Release(ReleaseEvent::new(self.blob.id())));
                    } else {
                        // normal cleanup, save manifest
                        events.push_front(CleanupEvent::Save(
                            self.blob.save(self.blob.id(), Arc::new(manifest)).1,
                        ));
                    }
                }
            }
        }
    }
}

/// Search engine cleanup event
#[derive(Debug)]
pub enum CleanupEvent {
    /// The index store requests storage save
    Release(ReleaseEvent),
    /// The index store requires storage load
    Load(LoadEvent),
    /// The index store requests storage save
    Save(SaveEvent),
}

#[cfg(test)]
mod tests {
    use crate::engine::Engine;

    #[test]
    fn cleanup_implements_send() {
        fn sendy(_send: impl Send) {}
        sendy(Engine::builder().build().cleanup())
    }
}
