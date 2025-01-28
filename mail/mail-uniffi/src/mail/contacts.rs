use super::MailUserSession;
use crate::errors::{ActionError, VoidActionResult};
use crate::{
    core::datatypes::{GroupedContacts, Id},
    uniffi_async, WatchHandle,
};
use crate::{watch_channel_inner, UniffiRecord};
use proton_core_common::models::Contact as RealContact;
use proton_core_common::utils::MapVec as _;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::MailContextError;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{task, time::interval};

/// Returns grouped contacts by the first grapheme of the name.
///
#[allow(clippy::missing_panics_doc)]
#[proton_uniffi_macros::export_result]
pub async fn contact_list(
    session: Arc<MailUserSession>,
) -> Result<Vec<GroupedContacts>, ActionError> {
    uniffi_async(async move {
        let tether = session.user_stash().connection();
        Result::<_, RealProtonMailError>::Ok(
            RealContact::contact_list(&tether)
                .await?
                .into_iter()
                .map(Into::into)
                .collect(),
        )
    })
    .await
    .map_err(ActionError::from)
}

#[allow(clippy::missing_panics_doc)]
#[uniffi::export]
pub async fn delete_contact(contact_id: Id, session: Arc<MailUserSession>) -> VoidActionResult {
    let user_context = session.ctx();
    uniffi_async(async move {
        RealContact::action_delete(user_context.action_queue(), vec![contact_id.into()])
            .await
            .map_err(MailContextError::from)?;

        Result::<_, RealProtonMailError>::Ok(())
    })
    .await
    .map_err(ActionError::from)
    .into()
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

#[proton_uniffi_macros::export_result]
pub async fn watch_contact_list(
    session: Arc<MailUserSession>,
    callback: Box<dyn ContactsLiveQueryCallback>,
) -> Result<WatchedContactList, ActionError> {
    let user_context = session.ctx();
    uniffi_async(async move {
        let callback = contacts_callback(session.clone(), callback);
        let (contact_list, handle) =
            RealContact::watch_contact_list(user_context.user_stash()).await?;

        let task_handle = watch_channel_inner(handle.receiver, callback);
        let watcher = Arc::new(WatchHandle::new(handle.handle, &task_handle));

        Result::<_, RealProtonMailError>::Ok(WatchedContactList {
            contact_list: contact_list.map_vec(),
            handle: watcher,
        })
    })
    .await
    .map_err(ActionError::from)
}

/// Obtains dampening function.
///
/// This returns a function that updates the boolean flag of whether we should
/// send an update which gets checked every `duration`.
///
pub fn contacts_callback(
    session: Arc<MailUserSession>,
    callback: Box<dyn ContactsLiveQueryCallback>,
) -> impl Fn() + Clone {
    let must_update = Arc::new(AtomicBool::new(false));
    let must_update_weak = Arc::downgrade(&must_update);

    tokio::spawn(async move {
        let mut interval = interval(Duration::from_millis(50));
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

                match contact_list {
                    ContactListResult::Ok(contact_list) => {
                        let callback_clone = callback.clone();

                        if task::spawn_blocking(move || callback_clone.on_update(contact_list))
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                    ContactListResult::Error(e) => {
                        tracing::error!("Failed to get contact list: {:?}", e);
                        return;
                    }
                }
            }
        }
    });

    move || must_update.store(true, Ordering::Relaxed)
}
