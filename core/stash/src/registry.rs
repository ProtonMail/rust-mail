//! This module provides functionality for managing the global [`Stash`]
//! registry.

use crate::stash::{Stash, StashError};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock, Weak};

/// Global registry instance.
pub(crate) static REGISTRY: OnceLock<Mutex<StashRegistry>> = OnceLock::new();

/// Registry entry for a [`Stash`].
struct RegistryEntry {
    /// Weak reference to the handle.
    handle: Weak<()>,

    /// Weak reference to the [`Stash`].
    stash: Weak<Stash>,
}

/// Global [`Stash`] registry.
///
/// The registry is used to store weak references to [`Stash`] instances, so
/// that they can be retrieved or created as needed. Crucially, it means that
/// attempting to create a [`Stash`] for a given path will always return the
/// same instance, rather than creating multiple instances for the same path,
/// preventing locking through mistakes.
///
/// Note: here, "same instance" means "the core things that make up a [`Stash`]
/// instance", as [`Stash`] instances are fully-cloneable and thread-safe.
///
pub(crate) struct StashRegistry {
    /// Map of paths to registry entries.
    entries: HashMap<PathBuf, RegistryEntry>,
}

impl StashRegistry {
    /// Creates a new [`StashRegistry`].
    pub(crate) fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Get or create a [`Stash`] for a given path.
    ///
    /// If a [`Stash`] already exists for the given path, it will be returned.
    ///
    /// If no [`Stash`] exists for the given path, a new [`Stash`] will be
    /// created and stored in the registry.
    ///
    /// # Parameters
    ///
    /// * `path` - Path to get or create a [`Stash`] for.
    ///
    pub(crate) fn get_or_create(&mut self, path: PathBuf) -> Result<Stash, StashError> {
        if let Some(entry) = self.entries.get(&path) {
            if let (Some(_handle), Some(stash)) = (entry.handle.upgrade(), entry.stash.upgrade()) {
                return Ok((*stash).clone());
            }
        }

        let stash = Arc::new(Stash::new(Some(&path))?);

        drop(self.entries.insert(
            path,
            RegistryEntry {
                handle: Arc::downgrade(&stash.handle),
                stash: Arc::downgrade(&stash),
            },
        ));

        Ok((*stash).clone())
    }

    /// Clean up dead entries.
    pub(crate) fn cleanup(&mut self) {
        self.entries
            .retain(|_, entry| entry.handle.upgrade().is_some() && entry.stash.upgrade().is_some());
    }
}
