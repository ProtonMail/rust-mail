use std::collections::BTreeSet;

use crate::actions::contacts::Delete as ContactsDelete;
use crate::datatypes::{GroupedContacts, Id, LabelId, Labels, LocalId, RemoteId};
use crate::models::{ContactCard, ContactEmail, ModelExtension};
use crate::{ContactError, CoreContextError, CoreContextResult};
use itertools::Itertools;
use proton_action_queue::queue::{ActionError, ActionOutput, Queue};
use proton_api_core::consts::General;
use proton_api_core::services::proton::requests::{GetContactsEmailsOptions, GetContactsOptions};
use proton_api_core::services::proton::response_data::{
    ContactBasic as ApiContactBasic, ContactFull as ApiContactFull,
};
use proton_api_core::services::proton::{Proton, ProtonCore};
use proton_api_core::SYNC_CONTACT_PAGE_SIZE;
use sqlite_watcher::watcher::TableObserver;
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, Stash, StashError, Tether, WatcherHandle};
use tracing::{debug, error};

#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("contacts")]
#[ModelActions(on_save)]
pub struct Contact {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    #[IdField(autoincrement)]
    pub local_id: Option<LocalId>,

    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    #[DbField]
    pub remote_id: Option<RemoteId>,

    /// Cards associated with the contact. They are in standard vCard format,
    /// although each field is kept separatly within new vCard.
    pub cards: Vec<ContactCard>,

    /// Emails associated with the contact.
    pub contact_emails: Vec<ContactEmail>,

    /// Creation time of the contact.
    #[DbField]
    pub create_time: u64,

    /// Labels associated with the contact. They are used to group contacts.
    #[DbField]
    pub label_ids: Labels,

    /// Last modification time of the contact.
    #[DbField]
    pub modify_time: u64,

    /// Name of the contact.
    #[DbField]
    pub name: String,

    /// Size of the contact.
    #[DbField]
    pub size: u64,

    /// Unique identifier of the contact.
    #[DbField]
    pub uid: RemoteId,

    /// Reflects whether the record has been deleted. This is used to ensure that
    /// delete happens in a two-step process, where the record is marked as
    /// deleted, then deleted from remote, then finally deleted from the local
    /// by event loop update.
    #[DbField]
    pub deleted: bool,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,
}

impl Contact {
    /// Save a contact to the database.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to
    /// ensure that existing conversations are updated.
    ///
    /// # Parameters
    ///
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///                 use for finding the records.
    ///
    /// # Errors
    ///
    /// Returns an error if the local conversation id is not set or the query
    /// failed.
    ///
    pub async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        if let Some(remote_id) = self.remote_id.clone() {
            if let Some(existing) = Self::find_by_id(remote_id, bond).await? {
                self.row_id = existing.row_id;
                self.local_id = existing.local_id;
            }
        } else if let Some(local_id) = self.local_id {
            if let Some(existing) = Self::find_by_id(local_id, bond).await? {
                self.row_id = existing.row_id;
                self.remote_id = existing.remote_id;
            }
        }

        <Self as Model>::save(self, bond).await
    }
    /// Returns the associated cards for a contact.
    ///
    /// This function retrieves the cards for a contact from the database,
    /// stores them in the contact struct, and then returns them.
    ///
    /// # Errors
    ///
    /// Returns a [`StashError`] if the cards cannot be retrieved.
    ///
    pub async fn cards(&mut self, tether: &Tether) -> Result<&Vec<ContactCard>, StashError> {
        self.cards = ContactCard::find(
            "WHERE remote_contact_id = ?",
            params![self.remote_id.clone()],
            tether,
        )
        .await?;

        Ok(&self.cards)
    }

    /// Returns the associated emails for a contact.
    ///
    /// This function retrieves the emails for a contact from the database,
    /// stores them in the contact struct, and then returns them.
    ///
    /// # Errors
    ///
    /// Returns a [`StashError`] if the emails cannot be retrieved.
    ///
    pub async fn emails(&mut self, tether: &Tether) -> Result<&Vec<ContactEmail>, StashError> {
        self.contact_emails = ContactEmail::find(
            "WHERE remote_contact_id = ? ORDER BY display_order ASC",
            params![self.remote_id.clone()],
            tether,
        )
        .await?;
        Ok(&self.contact_emails)
    }

    /// Extends [`Model::save()`] to set the contact id for children.
    ///
    /// # Errors
    ///
    /// See [`Model::save()`].
    ///
    pub async fn on_save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        for card in &mut self.cards {
            card.local_contact_id = self.local_id;
            card.remote_contact_id.clone_from(&self.remote_id);
        }
        for email in &mut self.contact_emails {
            email.local_contact_id = self.local_id;
            email.remote_contact_id.clone_from(&self.remote_id);
        }
        bond.execute(
            "DELETE FROM contact_cards WHERE local_contact_id = ?",
            params![self.local_id],
        )
        .await?;
        for card in &mut self.cards {
            card.local_id = None;
            card.row_id = None;
            card.save(bond).await.map_err(|e| {
                error!("Failed to update contact cards: {e}");
                e
            })?;
        }
        Ok(())
    }

    /// Updates all user contacts including their emails without their cards.
    ///
    /// The update includes a reset of the database.
    ///
    /// # Parameters
    ///
    /// * `api`   - The API instance to use to download the addresses.
    /// * `stash` - The database instance to store the addresses.
    ///
    /// # Errors
    ///
    /// Errors when the API request fails or when the database query fails.
    ///
    #[allow(clippy::too_many_lines)]
    pub async fn sync(api: &Proton, stash: &Stash) -> CoreContextResult<()> {
        macro_rules! request_pages {
            ($api: expr, $type: tt, $field: tt, $api_rq:tt, $options_type: tt) => {{
                let mut retval = vec![];
                let mut page_index = 0;
                debug!("Syncing partial {}", stringify!($field));
                loop {
                    let response = $api
                        .$api_rq($options_type {
                            page: page_index,
                            page_size: SYNC_CONTACT_PAGE_SIZE,
                            ..Default::default()
                        })
                        .await
                        .map_err(|err| {
                            error!(
                                "Failed to sync {} for page {page_index}: {err}",
                                stringify!($field)
                            );

                            err
                        })?;

                    let is_last_request = response.$field.len() < SYNC_CONTACT_PAGE_SIZE;

                    debug!(
                        "Synced page {} of partial {}, {} {} fetched",
                        page_index,
                        stringify!($field),
                        response.$field.len(),
                        stringify!($field),
                    );

                    retval.extend(response.$field.into_iter().map($type::from).collect_vec());

                    if is_last_request {
                        break;
                    }

                    page_index += 1;
                }

                CoreContextResult::<Vec<$type>>::Ok(retval)
            }};
        }

        let api_clone = api.clone();
        let contacts_handle = tokio::spawn(async move {
            request_pages!(
                api_clone,
                Contact,
                contacts,
                get_contacts,
                GetContactsOptions
            )
        });

        let api_clone = api.clone();
        let contact_emails_handle = tokio::spawn(async move {
            request_pages!(
                api_clone,
                ContactEmail,
                contact_emails,
                get_contacts_emails,
                GetContactsEmailsOptions
            )
        });

        #[allow(clippy::items_after_statements)]
        fn map_err<T, E1, E2>(res: Result<Result<T, E1>, E2>) -> CoreContextResult<T>
        where
            E1: Into<CoreContextError>,
            E2: Into<CoreContextError>,
        {
            res.map_err(Into::into).and_then(|r| r.map_err(Into::into))
        }

        let (contacts, contact_emails) = tokio::join!(contacts_handle, contact_emails_handle);
        let (contacts, contact_emails) = (map_err(contacts)?, map_err(contact_emails)?);

        let mut conn = stash.connection();
        let tx = conn.transaction().await?;
        // Reset the database state by deleting all contacts.
        tx.execute("DELETE FROM contacts", vec![]).await?;
        tx.execute("DELETE FROM contact_emails", vec![]).await?;
        tx.execute("DELETE FROM contact_cards", vec![]).await?;
        tx.execute("DELETE FROM contact_email_labels", vec![])
            .await?;

        for mut contact in contacts {
            contact.save(&tx).await?;
        }

        for mut contact_email in contact_emails {
            contact_email.save(&tx).await?;
        }

        tx.commit().await?;

        Ok(())
    }

    /// Updates the full contact with the given ID including its emails and
    /// cards.
    ///
    /// # Parameters
    ///
    /// * `id`    - The ID of the [`Contact`] to sync.
    /// * `api`   - The API instance to use to download the addresses.
    /// * `stash` - The database instance to store the addresses.
    ///
    /// # Errors
    ///
    /// Errors when the API request fails or when the database query fails.
    ///
    pub async fn sync_with_card(
        local_id: LocalId,
        api: &Proton,
        bond: &Bond<'_>,
    ) -> CoreContextResult<()> {
        debug!("Syncing full contact for contact id {local_id}");
        let remote_id = local_id
            .counterpart::<Contact>(bond)
            .await?
            .ok_or_else(|| {
                CoreContextError::ContactError(ContactError::ContactDoesNotHaveRemoteId(local_id))
            })?;

        let mut contact_with_card = Contact::from(
            api.get_contact(remote_id.clone().into())
                .await
                .map_err(|err| {
                    error!("Failed to fetch full contact with id {local_id}: {err}");
                    err
                })?
                .contact,
        );

        contact_with_card.save(bond).await.map_err(|err| {
            error!("Failed to sync full contact to db: {err}");
            err
        })?;

        for email in &mut contact_with_card.contact_emails {
            email.save(bond).await.map_err(|e| {
                error!("Failed to update contact emails: {e}");
                e
            })?;
        }
        Ok(())
    }

    /// Returns a list of contacts grouped by the first letter of their name.
    ///
    /// # Parameters
    ///
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///
    /// # Errors
    ///
    /// when querying the database fails.
    ///
    pub async fn contact_list(tether: &Tether) -> Result<Vec<GroupedContacts>, StashError> {
        let mut contacts = Contact::find("WHERE deleted = 0", vec![], tether).await?;

        for contact in &mut contacts {
            contact.emails(tether).await?;
        }

        Ok(GroupedContacts::from_contacts(contacts))
    }

    pub async fn action_delete(
        queue: &Queue,
        contact_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<ContactsDelete>, ActionError<ContactsDelete>> {
        let action = ContactsDelete::new(contact_ids);
        queue.apply_action(action).await
    }

    /// Marks a contact as deleted.
    /// Deletion is two-step process: first, the record is marked as deleted in
    /// the database, then it is deleted from the remote server, and finally
    /// It is deleted from the local database by the event loop update.
    ///
    pub async fn mark_delete(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        self.deleted = true;
        self.save(bond).await
    }

    /// Marks a contact as undeleted.
    /// This method serves as the reverse of [`Contact::mark_delete()`].
    /// which can revert the deletion of a contact in case of something unpredictable happend.
    ///
    pub async fn mark_undelete(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        self.deleted = false;
        self.save(bond).await
    }

    pub async fn delete_from_remote(
        remote_ids: &[RemoteId],
        api: &Proton,
    ) -> CoreContextResult<Vec<RemoteId>> {
        let response = api
            .put_delete_contacts(remote_ids.iter().cloned().map_into().collect())
            .await?;

        Ok(response
            .responses
            .iter()
            .filter(|r| r.response.code != General::NoError as u32)
            .map(|r| r.id.clone().into())
            .collect())
    }

    pub async fn watch_contact_list(
        stash: &Stash,
    ) -> Result<(Vec<GroupedContacts>, WatcherHandle), StashError> {
        let handle = stash.subscribe_to(|sender| Box::new(ContactListWatcher { sender }))?;
        let tether = stash.connection();
        let contacts = Contact::contact_list(&tether).await?;

        Ok((contacts, handle))
    }

    // pub async fn vcard<Provider: PGPProviderSync>(
    //     &mut self,
    //     pgp_provider: &Provider,
    //     unlocked_user_keys: &UnlockedUserKeys<Provider>,
    // ) -> CoreContextResult<VCard> {
    //     self.cards().await?;

    //     VCard::new(pgp_provider, unlocked_user_keys, self)
    // }
}

impl From<ApiContactBasic> for Contact {
    fn from(value: ApiContactBasic) -> Self {
        Self {
            local_id: None,
            remote_id: Some(value.id.into()),
            cards: vec![],
            contact_emails: vec![],
            create_time: value.create_time,
            label_ids: Labels::new(value.label_ids.into_iter().map(LabelId::from).collect()),
            modify_time: value.modify_time,
            name: value.name,
            size: value.size,
            uid: value.uid.into(),
            deleted: false,
            row_id: None,
        }
    }
}

#[cfg(any(test, debug_assertions))]
impl Default for Contact {
    #[allow(clippy::default_trait_access)]
    fn default() -> Self {
        Self {
            local_id: Default::default(),
            remote_id: Default::default(),
            cards: Default::default(),
            contact_emails: Default::default(),
            create_time: Default::default(),
            label_ids: Default::default(),
            modify_time: Default::default(),
            name: Default::default(),
            size: Default::default(),
            uid: RemoteId::from(String::default()),
            deleted: Default::default(),
            row_id: Default::default(),
        }
    }
}

impl From<ApiContactFull> for Contact {
    fn from(value: ApiContactFull) -> Self {
        Self {
            local_id: None,
            remote_id: Some(value.id.into()),
            cards: value.cards.into_iter().map(ContactCard::from).collect(),
            contact_emails: value
                .contact_emails
                .into_iter()
                .map(ContactEmail::from)
                .collect(),
            create_time: value.create_time,
            label_ids: Labels::new(value.label_ids.into_iter().map(LabelId::from).collect()),
            modify_time: value.modify_time,
            name: value.name,
            size: value.size,
            uid: value.uid.into(),
            deleted: false,
            row_id: None,
        }
    }
}

pub struct ContactListWatcher {
    sender: flume::Sender<()>,
}

impl TableObserver for ContactListWatcher {
    fn tables(&self) -> Vec<String> {
        vec![
            Contact::table_name().to_string(),
            ContactEmail::table_name().to_string(),
        ]
    }

    fn on_tables_changed(&self, _changed_tables: &BTreeSet<String>) {
        self.sender
            .send(())
            .inspect_err(|e| error!("Failed to send notification for ContactListWatcher: {e}"))
            .ok();
    }
}
