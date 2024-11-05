use crate::{spawn_async, utils::DAMPENING_PERIOD, UniffiRecord};
use proton_core_common::models::Contact as RealContact;
use proton_mail_common::MailContextError;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{task, time::interval};

use crate::{
    core::datatypes::{GroupedContacts, Id},
    uniffi_async, WatchHandle,
};

use super::{MailSessionError, MailUserSession, MailboxError};

/// Returns grouped contacts by the first grapheme of the name.
///
#[allow(clippy::missing_panics_doc)]
#[uniffi::export]
pub async fn contact_list(
    session: Arc<MailUserSession>,
) -> Result<Vec<GroupedContacts>, MailboxError> {
    uniffi_async(async move {
        Ok(RealContact::contact_list(session.user_stash())
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
    })
    .await
}

#[allow(clippy::missing_panics_doc)]
#[uniffi::export]
pub async fn delete_contact(
    contact_id: Id,
    session: Arc<MailUserSession>,
) -> Result<(), MailSessionError> {
    let user_context = session.ctx();
    uniffi_async(async move {
        RealContact::action_delete(
            user_context.session(),
            user_context.queue(),
            vec![contact_id.into()],
        )
        .await
        .map_err(MailContextError::from)?;

        Ok(())
    })
    .await
}

/// A callback interface for live queries.
///
/// This interface is used to notify the client when observed data has been
/// updated.
///
#[uniffi::export(callback_interface)]
pub trait ContactsLiveQueryCallback: Send + Sync {
    /// Notify the client that the observed data has been updated.
    ///
    /// This method is called when the observed data has been updated. It does
    /// not provide any information about the update, but the client can use
    /// this as a signal to refresh its view of the data.
    ///
    fn on_update(&self, contacts: Vec<GroupedContacts>);
}

#[derive(UniffiRecord)]
pub struct WatchedContactList {
    contact_list: Vec<GroupedContacts>,
    handle: Arc<WatchHandle>,
}

#[uniffi::export]
pub async fn watch_contact_list(
    session: Arc<MailUserSession>,
    callback: Box<dyn ContactsLiveQueryCallback>,
) -> Result<WatchedContactList, MailboxError> {
    let user_context = session.ctx();
    uniffi_async(async move {
        let callback = damp_contacts_callback(session.clone(), callback);
        let watcher = WatchHandle::new();
        let watcher_clone = watcher.clone();
        let (contact_list, channel) =
            RealContact::watch_contact_list(user_context.user_stash()).await?;

        drop(spawn_async(async move {
            loop {
                if watcher_clone.should_stop() {
                    return;
                }

                if channel.recv_async().await.is_err() {
                    return;
                }

                callback();
            }
        }));

        Ok(WatchedContactList {
            contact_list: contact_list.into_iter().map(Into::into).collect(),
            handle: Arc::new(watcher),
        })
    })
    .await
}

/// Obtains dampening function.
///
/// This returns a function that updates the boolean flag of whether we should
/// send an update which gets checked every `duration`.
///
pub fn damp_contacts_callback(
    session: Arc<MailUserSession>,
    callback: Box<dyn ContactsLiveQueryCallback>,
) -> impl Fn() + Clone {
    let must_update = Arc::new(AtomicBool::new(false));
    let must_update_weak = Arc::downgrade(&must_update);

    tokio::spawn(async move {
        let mut interval = interval(Duration::from_millis(DAMPENING_PERIOD));
        let callback = Arc::new(callback);

        loop {
            interval.tick().await;
            let Some(must_update) = must_update_weak.upgrade() else {
                return;
            };
            // If there's something in there we call on_update and set false
            // If there isn't we set false either way
            if must_update.swap(false, Ordering::Relaxed) {
                let contact_list = contact_list(session.clone()).await;

                if contact_list.is_err() {
                    return;
                }

                let callback_clone = callback.clone();

                if task::spawn_blocking(move || callback_clone.on_update(contact_list.unwrap()))
                    .await
                    .is_err()
                {
                    return;
                }
            }
        }
    });

    move || must_update.store(true, Ordering::Relaxed)
}
