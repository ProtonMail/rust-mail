use super::MailUserSession;
use crate::errors::{MailErrorKind, ProtonMailError, VoidProtonMailResult};
use crate::{
    core::datatypes::{GroupedContacts, Id},
    uniffi_async, WatchHandle,
};
use crate::{spawn_async, utils::DAMPENING_PERIOD, UniffiRecord};
use proton_core_common::models::Contact as RealContact;
use proton_mail_common::errors::{MailErrorDetails as RealMailErrorDetails, Reason};
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
) -> Result<Vec<GroupedContacts>, ProtonMailError> {
    uniffi_async(async move {
        Result::<_, RealMailErrorDetails>::Ok(
            RealContact::contact_list(session.user_stash())
                .await?
                .into_iter()
                .map(Into::into)
                .collect(),
        )
    })
    .await
    .map_err(|details| MailErrorKind::UserActionError.with(details))
}

#[allow(clippy::missing_panics_doc)]
#[uniffi::export]
pub async fn delete_contact(contact_id: Id, session: Arc<MailUserSession>) -> VoidProtonMailResult {
    let user_context = session.ctx();
    uniffi_async(async move {
        RealContact::action_delete(
            user_context.session(),
            user_context.queue(),
            vec![contact_id.into()],
        )
        .await
        .map_err(MailContextError::from)?;

        Result::<_, RealMailErrorDetails>::Ok(())
    })
    .await
    .map_err(|details| MailErrorKind::UserActionError.with(details))
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
) -> Result<WatchedContactList, ProtonMailError> {
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

        Result::<_, RealMailErrorDetails>::Ok(WatchedContactList {
            contact_list: contact_list.into_iter().map(Into::into).collect(),
            handle: Arc::new(watcher),
        })
    })
    .await
    .map_err(|details| MailErrorKind::UserActionError.with(details))
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
        let dampening_period = DAMPENING_PERIOD.lock().await.next().unwrap();
        let mut interval = interval(Duration::from_millis(dampening_period));
        let callback = Arc::new(callback);

        loop {
            interval.tick().await;
            let Some(must_update) = must_update_weak.upgrade() else {
                return;
            };
            // If there's something in there we call on_update and set false
            // If there isn't we set false either way
            if must_update.swap(false, Ordering::Relaxed) {
                interval.tick().await;
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
