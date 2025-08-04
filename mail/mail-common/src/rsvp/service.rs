use super::{RsvpCache, RsvpContacts};
use stash::stash::Stash;

/// Service that provides RSVP functionality, combining cache and contacts management.
///
/// This service is only available for the main app (Origin::App) as RSVP functionality
/// is not needed in share extensions.
pub struct RsvpService {
    cache: RsvpCache,
    contacts: RsvpContacts,
}

impl RsvpService {
    /// Creates a new RsvpService with the given stash for contacts storage.
    pub fn new(stash: &Stash) -> Self {
        Self {
            cache: Default::default(),
            contacts: RsvpContacts::new(stash),
        }
    }

    /// Returns a reference to the RSVP cache.
    pub(crate) fn cache(&self) -> &RsvpCache {
        &self.cache
    }

    /// Returns a reference to the RSVP contacts manager.
    pub(crate) fn contacts(&self) -> &RsvpContacts {
        &self.contacts
    }
}
