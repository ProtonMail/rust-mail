use super::MailUserSession;
use crate::core::datatypes::{ContactGroupItem, ContactSuggestions};
use crate::errors::{ActionError, VoidActionResult};
use crate::{UniffiRecord, watch_channel_inner};
use crate::{
    WatchHandle,
    core::datatypes::{DeviceContact, GroupedContacts, Id},
    uniffi_async,
};
use futures::future::try_join_all;
use itertools::Itertools;
use proton_core_common::datatypes::DeviceContact as RealDeviceContact;
use proton_core_common::models::{AppSettings, Contact as RealContact};
use proton_core_common::utils::MapVec as _;
use proton_mail_common::ProtonMailError as RealProtonMailError;
use proton_mail_common::{MailContextError, MailUserContext};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{task, time::interval};

/// Returns grouped contacts by the first grapheme of the name.
///
#[uniffi_export]
pub async fn contact_list(
    session: Arc<MailUserSession>,
) -> Result<Vec<GroupedContacts>, ActionError> {
    let stash = session.user_stash()?;
    uniffi_async(async move {
        let tether = stash.connection().await?;
        Ok::<_, RealProtonMailError>(
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

/// Returns a specific contact group detailed info
// This is not necessary but android wants this.
#[uniffi_export]
pub async fn contact_group_by_id(
    session: Arc<MailUserSession>,
    id: Id,
) -> Result<ContactGroupItem, ActionError> {
    let stash = session.user_stash()?;
    uniffi_async(async move {
        let tether = stash.connection().await?;
        Ok::<_, RealProtonMailError>(
            RealContact::contact_group_by_id(&tether, id.into())
                .await?
                .into(),
        )
    })
    .await
    .map_err(ActionError::from)
}

/// Returns a list of contact suggestions (used for example in Composer).
///
/// If the `AppSettings::use_combine_contacts` is set, the function will include
/// all other available contacts from all logged in accounts.
///
/// Contacts are sorted, deduplicated but not filtered by the query.
/// Contacts from other accounts have lower priority and will appear at the end of the list.
///
#[uniffi_export]
pub async fn contact_suggestions(
    device_contacts: Vec<DeviceContact>,
    session: Arc<MailUserSession>,
) -> Result<Arc<ContactSuggestions>, ActionError> {
    let ctx = session.ctx()?.clone();
    uniffi_async(async move {
        let tether = ctx.user_stash().connection().await?;
        let primary_contacts = RealContact::contact_suggestions(
            device_contacts
                .into_iter()
                .map_into::<RealDeviceContact>()
                .collect(),
            &tether,
        );
        let acc_tether = ctx
            .mail_context()
            .core_context()
            .account_stash()
            .connection()
            .await?;
        let app_settings = AppSettings::get_or_default(&acc_tether).await;
        let other_acc_contacts = if app_settings.use_combine_contacts {
            let other_user_ctxs = ctx.other_mail_user_ctxs().await?;
            let iter = other_user_ctxs.iter().map(|ctx| async {
                let tether = ctx.user_stash().connection().await?;
                let contacts = RealContact::contact_suggestions(vec![], &tether).await?;

                Result::<_, MailContextError>::Ok(contacts)
            });

            try_join_all(iter).await?
        } else {
            vec![]
        };
        let mut primary_contacts = primary_contacts.await?;

        for other in other_acc_contacts {
            primary_contacts.concat(other);
        }

        Result::<_, RealProtonMailError>::Ok(Arc::new(primary_contacts.into()))
    })
    .await
    .map_err(ActionError::from)
}

#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn delete_contact(
    contact_id: Id,
    session: Arc<MailUserSession>,
) -> Result<(), ActionError> {
    let user_context = session.ctx()?;
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

#[uniffi_export]
pub async fn watch_contact_list(
    session: Arc<MailUserSession>,
    callback: Box<dyn ContactsLiveQueryCallback>,
) -> Result<WatchedContactList, ActionError> {
    let user_context = session.ctx()?;
    uniffi_async(async move {
        let callback = contacts_callback(&user_context, session, callback);

        let (contact_list, handle) =
            RealContact::watch_contact_list(user_context.user_stash()).await?;

        let task_handle = watch_channel_inner(&*user_context, handle.receiver, callback);
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
    user_ctx: &MailUserContext,
    session: Arc<MailUserSession>,
    callback: Box<dyn ContactsLiveQueryCallback>,
) -> impl Fn() + Clone + use<> {
    let must_update = Arc::new(AtomicBool::new(false));
    let must_update_weak = Arc::downgrade(&must_update);

    user_ctx.spawn(async move {
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
                let contact_list = contact_list(Arc::clone(&session)).await;

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
