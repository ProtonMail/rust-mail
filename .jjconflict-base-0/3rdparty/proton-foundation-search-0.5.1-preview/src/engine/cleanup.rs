use std::fmt::Debug;
use std::sync::atomic::Ordering;

use super::*;
use crate::transaction::{LoadEvent, SaveEvent, TransactionState};

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
impl Engine {
    /// Cleanup the engine's released blobs.
    /// This is intentionally a separate write transaction to avoid race conditions with readers trying to load removed blobs.
    pub fn cleanup(&self) -> Option<Cleanup> {
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
        let mut modified = false;
        Some(Cleanup(Box::new(std::iter::from_fn(move || {
            // guard will release writer reservation on drop
            let _guard = &guard;
            let mut state = tran.take()?;
            match state.load()? {
                Ok(manifest) => {
                    match manifest.released_blobs.pop_first() {
                        Some(blob) => {
                            // keep state for later
                            tran = Some(state);
                            modified = true;
                            Some(CleanupEvent::Release(blob))
                        }
                        None if std::mem::take(&mut modified) => {
                            Some(CleanupEvent::Save(state.save().0))
                        }
                        None => None,
                    }
                }
                Err(load) => {
                    // keep the state for saving later
                    tran = Some(state);
                    Some(CleanupEvent::Load(load))
                }
            }
        }))))
    }
}

/// Iterator of the cleanum transaction events
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
pub struct Cleanup(pub(super) Box<dyn Iterator<Item = CleanupEvent>>);

impl Iterator for Cleanup {
    type Item = CleanupEvent;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

/// Search engine cleanup event
#[derive(Debug)]
pub enum CleanupEvent {
    /// The index store requests storage save
    Release(Box<str>),
    /// The index store requires storage load
    Load(LoadEvent),
    /// The index store requests storage save
    Save(SaveEvent),
}
