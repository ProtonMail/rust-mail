use parking_lot::RwLock;
use proton_core_api::services::proton::UserId;
use proton_mail_common::MailUserContext;
use std::collections::HashMap;
use std::sync::{Arc, Weak};

/// Holds strong references to all active user context instances
/// and hands out weak pseudo-pointers to them.
///
/// The main purpose of this type is to prevent user context instances from
/// dangling inside view models within the GUI. We've had issues in the past
/// where certain Swift view models would leak instances of the `Sidebar` and
/// `Mailbox` on logout. These objects internally held strong references
/// to the user context, preventing it from being dropped and in turn preventing
/// the user from logging back in.
///
/// This type is a workaround to that issue. It allows us to directly control the
/// lifetime of user context instances, handing out weak references to them instead.
pub struct MailUserContextMap {
    this: Weak<Self>,
    map: RwLock<HashMap<UserId, Arc<MailUserContext>>>,
}

impl MailUserContextMap {
    pub fn new() -> Arc<Self> {
        Arc::new_cyclic(|this| Self {
            this: Weak::clone(this),
            map: RwLock::default(),
        })
    }

    /// Insert the user context into the map, returning a weak pseudo-pointer to it.
    pub fn insert(&self, ctx: Arc<MailUserContext>) -> MailUserContextPtr {
        let id = ctx.user_id().to_owned();

        self.map.write().insert(id.clone(), ctx);

        MailUserContextPtr::new(Weak::clone(&self.this), id)
    }

    /// Remove the user context from the map if it exists.
    pub fn remove(&self, user_id: &UserId) -> Option<Arc<MailUserContext>> {
        self.map.write().remove(user_id)
    }

    /// Clear the map.
    ///
    /// This is used to clear the map when all sessions are deleted.
    ///
    pub fn clear(&self) {
        self.map.write().clear();
    }

    /// Get the first user context in the map.
    ///
    /// Helpful when we want whatever user context from the map.
    ///
    pub fn first(&self) -> Option<Arc<MailUserContext>> {
        self.map.read().values().next().cloned()
    }

    fn get(&self, user_id: &UserId) -> Option<Arc<MailUserContext>> {
        self.map.read().get(user_id).map(Arc::clone)
    }
}

/// Acts as a kind of weak pointer to a mail user context.
#[derive(Clone)]
pub struct MailUserContextPtr {
    ctx: Weak<MailUserContextMap>,
    id: Arc<UserId>,
}

impl MailUserContextPtr {
    fn new(ctx: Weak<MailUserContextMap>, id: UserId) -> Self {
        Self {
            ctx,
            id: Arc::new(id),
        }
    }

    /// Upgrade the pseudo-pointer to a strong reference if it still exists.
    /// This is non-destructive; the pseudo-pointer will still be upgradeable.
    pub fn upgrade(&self) -> Option<Arc<MailUserContext>> {
        self.ctx.upgrade().and_then(|ctx| ctx.get(&self.id))
    }

    /// Upgrade the pseudo-pointer to a strong reference if it still exists.
    /// This is destructive; the pseudo-pointer will no longer be upgradeable.
    pub fn consume(&self) -> Option<Arc<MailUserContext>> {
        self.ctx.upgrade().and_then(|ctx| ctx.remove(&self.id))
    }
}
