use crate::utils::MapVec as _;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::future::Future;
use std::iter;
use std::time::Instant;

use crate::actions::contacts::Delete as ContactsDelete;
use crate::datatypes::{
    ContactSuggestion, DeviceContact, GroupedContacts, LabelType, Labels, LocalContactId,
};
use crate::models::{ContactCard, ContactEmail, ModelExtension, ModelIdExtension};
use crate::{ContactError, CoreContextError, CoreContextResult};
use futures::future::try_join;
use futures::try_join;
use indoc::formatdoc;
use itertools::Itertools;
use proton_action_queue::queue::{ActionError, ActionOutput, Queue};
use proton_api_core::consts::General;
use proton_api_core::services::proton::common::ContactId;
use proton_api_core::services::proton::prelude::ContactUID;
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
use tokio::task::JoinSet;
use tracing::{debug, error};

use super::Label;

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
    pub local_id: Option<LocalContactId>,

    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    #[DbField]
    pub remote_id: Option<ContactId>,

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
    pub uid: ContactUID,

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

impl ModelIdExtension for Contact {
    type RemoteId = ContactId;
    fn remote_id(&self) -> Option<&Self::RemoteId> {
        self.remote_id.as_ref()
    }
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
            if let Some(existing) = Self::find_by_remote_id(remote_id, bond).await? {
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
    /// You might have noticed that this function returns another future. This future, when polled
    /// will reset the database AND store all of the contacts.
    ///
    /// This future MUST ONLY be polled after syncing contact labels.
    /// FIXME: Assert this invariant via the type system in 1.85
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
    #[tracing::instrument(skip(api, stash))]
    #[allow(clippy::too_many_lines)]
    #[must_use]
    pub async fn sync(
        api: &Proton,
        stash: &Stash,
    ) -> CoreContextResult<impl Future<Output = CoreContextResult<()>>> {
        // In order to maximize throughput we do as follows:
        // 1. We download the first batch
        // 2. We calculate how many batches are left and request them all in parallel.
        // 3. When all of the batches arrive we store them in the database efficiently. This is, without
        //    going through the on_save method and calling `[Model::save]` diretly which performs too many
        //    queries. Previously nlogn, now n.
        //    This is fine because
        //    * We empty the database beforehand
        //    * We don't update any record
        //    * We manually map the ContactId from the contact to the ContactEmail.

        let t0 = Instant::now();
        let (first_contacts, first_emails) = try_join(
            api.get_contacts(GetContactsOptions {
                page: 0,
                page_size: SYNC_CONTACT_PAGE_SIZE,
                ..Default::default()
            }),
            api.get_contacts_emails(GetContactsEmailsOptions {
                page: 0,
                page_size: SYNC_CONTACT_PAGE_SIZE,
                ..Default::default()
            }),
        )
        .await?;
        debug!("Requested initial batch in {:?}", t0.elapsed());

        let mut contacts_joinset = JoinSet::new();
        let mut emails_joinset = JoinSet::new();

        let page = SYNC_CONTACT_PAGE_SIZE as u64;
        if let Some(rem) = first_contacts.total.checked_sub(page) {
            let rem = rem.div_ceil(page);
            debug!("Requesting {rem} batches for contacts");
            for page in 1..=rem {
                let api = api.clone();
                contacts_joinset.spawn(async move {
                    api.get_contacts(GetContactsOptions {
                        page,
                        page_size: SYNC_CONTACT_PAGE_SIZE,
                        ..Default::default()
                    })
                    .await
                    .map(|x| x.contacts)
                });
            }
        }

        if let Some(rem) = first_emails.total.checked_sub(page) {
            let rem = rem.div_ceil(page);
            debug!("Requesting {rem} batches for emails");
            for page in 1..=rem {
                let api = api.clone();
                emails_joinset.spawn(async move {
                    api.get_contacts_emails(GetContactsEmailsOptions {
                        page,
                        page_size: SYNC_CONTACT_PAGE_SIZE,
                        ..Default::default()
                    })
                    .await
                    .map(|x| x.contact_emails)
                });
            }
        }

        let contacts = contacts_joinset.join_all().await;
        let contacts = iter::once(Ok(first_contacts.contacts)).chain(contacts);

        let emails = emails_joinset.join_all().await;
        let emails = iter::once(Ok(first_emails.contact_emails)).chain(emails);

        debug!("Downloaded all contacts in {:?}", t0.elapsed());

        let mut tether = stash.connection();
        // We are splitting the store and download functions in two so that it's faster.
        Ok(async move {
            // Let's start with a clean database
            let tx = tether.transaction().await?;
            tx.execute("DELETE FROM contacts", vec![]).await?;
            tx.execute("DELETE FROM contact_emails", vec![]).await?;
            tx.execute("DELETE FROM contact_cards", vec![]).await?;
            tx.execute("DELETE FROM contact_email_labels", vec![])
                .await?;

            // We will use this to map the contact_emails to the contacts without having to
            // query the db each time we instert one.
            // We require this to happen since the contact_emails need the local id of its contact.
            let mut id_map = HashMap::new();

            let t = Instant::now();
            for (page, contact_page) in contacts.enumerate() {
                let t_inner = Instant::now();
                for contact in contact_page? {
                    let mut contact = Contact::from(contact);
                    <Contact as Model>::save(&mut contact, &tx).await?;
                    id_map.insert(contact.remote_id.unwrap(), contact.local_id.unwrap());
                }
                debug!("stored contacts page {page} in {:?}", t_inner.elapsed());
            }
            debug!(
                "Stored {} contacts to the db in {:?}",
                id_map.len(),
                t.elapsed()
            );

            let mut count = 0;
            let t = Instant::now();
            for (page, email_page) in emails.enumerate() {
                let t_inner = Instant::now();
                for em in email_page? {
                    let Some(local_id) = id_map.get(&em.contact_id) else {
                        error!("a contact_email has no contact");
                        continue;
                    };
                    count += 1;
                    let mut email = ContactEmail::from(em);
                    email.local_contact_id = Some(*local_id);
                    <ContactEmail as Model>::save(&mut email, &tx).await?;
                }
                debug!(
                    "stored contact_emails page {page} in {:?}",
                    t_inner.elapsed()
                );
            }

            debug!(
                "Stored {count} contacts_emails to the db in {:?}",
                t.elapsed()
            );
            tx.commit().await?;
            debug!("Synced all contacts in {:?}", t0.elapsed());
            Ok::<(), CoreContextError>(())
        })
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
        local_id: LocalContactId,
        api: &Proton,
        bond: &Bond<'_>,
    ) -> CoreContextResult<()> {
        debug!("Syncing full contact for contact id {local_id}");
        let remote_id = Contact::local_id_counterpart(local_id, bond)
            .await?
            .ok_or_else(|| {
                CoreContextError::ContactError(ContactError::ContactDoesNotHaveRemoteId(local_id))
            })?;

        let mut contact_with_card = Contact::from(
            api.get_contact(remote_id.clone())
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
        let (mut contacts, contact_groups) = try_join!(
            Contact::find("WHERE deleted = 0", vec![], tether),
            Label::find_by_kind(LabelType::ContactGroup, tether)
        )?;

        for contact in &mut contacts {
            contact.emails(tether).await?;
        }

        Ok(GroupedContacts::from_contacts_and_groups(
            contacts,
            contact_groups,
        ))
    }

    /// Returns a list of contact suggestions (used for example in Composer). Sorted, deduplicated and filtered by the query.
    ///
    /// # Parameters
    ///
    /// * `query` - a plaintext string provided by the user that we need to complete
    /// * `device_contacts` - contacts stored in the device storage, not shared between proton clients.
    /// * `tether` - The database interface
    ///
    /// # Errors
    ///
    /// when querying the database fails.
    ///
    /// # Panics
    ///
    /// This function panics if remote ID of the contact is missing.
    ///
    #[allow(trivial_casts)] // Box<dyn ToSql> cannot be infered
    pub async fn contact_suggestions(
        query: &str,
        device_contacts: Vec<DeviceContact>,
        tether: &Tether,
    ) -> Result<Vec<ContactSuggestion>, StashError> {
        let query = query.trim();
        let query = query.to_lowercase();

        // Early exit heurestic
        if query.is_empty() {
            return Ok(Vec::new());
        }

        // 1. Get contact groups that are matching the query.
        // That matching is case insensitive
        // TODO (ET-1971): Filter by name in SQL
        let contact_groups = Label::find(
            "WHERE label_type = ? ORDER BY display_order ASC",
            params![LabelType::ContactGroup],
            tether,
        )
        .await?
        .into_iter()
        .filter(|group| group.name.to_lowercase().contains(&query))
        .collect::<Vec<_>>();

        let group_label_ids = contact_groups
            .iter()
            .filter_map(|group| group.remote_id.clone())
            .collect::<HashSet<_>>();

        // 2. Get contact emails that are either matching query or are part of matched groups
        // TODO (ET-1971): Filter by name in SQL
        let contact_emails: Vec<ContactEmail> = ContactEmail::all(tether)
            .await?
            .into_iter()
            .filter(|email: &ContactEmail| {
                // We have to repeat the filter from SQL and add name.
                email.name.as_str().to_lowercase().contains(&query)
                    || email.email.as_str().to_lowercase().contains(&query)
                    || email
                        .label_ids
                        .iter()
                        .any(|id| group_label_ids.contains(id))
            })
            .collect();

        let remote_contact_ids = contact_emails
            .iter()
            .filter_map(|contact_email| contact_email.remote_contact_id.clone())
            .collect::<HashSet<_>>();

        // TODO (ET-1971): Filter contacts in SQLite
        let mut contacts = Contact::find(formatdoc!("WHERE deleted = 0 ",), vec![], tether)
            .await?
            .into_iter()
            .filter(|contact: &Contact| {
                contact.name.to_lowercase().contains(&query)
                // Even if the contact doesn't match the query, the email address associated with the contact might
                // Example:
                // Contact: "Bar" <foo@pm.me>
                //
                // I search for "foo"
                //
                // "Bar" isn't matched but "foo@pm.me" is.
                    || remote_contact_ids.contains(contact.remote_id.as_ref().unwrap())
            })
            .collect::<Vec<Contact>>();

        for contact in &mut contacts {
            // TODO (ET-1971): Even though we just loaded contact emails,
            // we already filtered them.
            // However, there is a case where contact emails did not match query, but contact name did.
            // In that case we still need to load contact email.
            // That double fetching could go away if we fetch ConctactEmail by using custom SQL query with
            // some inner join, but it has to wait for proper unicode filtering in SQLite.
            // For now its better to be correct but inefficient
            contact.emails(tether).await?;
        }

        let device_contacts = device_contacts
            .into_iter()
            .filter(|contact| {
                contact.name.to_lowercase().contains(&query)
                    || contact
                        .emails
                        .iter()
                        .any(|email| email.to_lowercase().contains(&query))
            })
            .collect();

        let suggestions = ContactSuggestion::from_contacts_and_device_contacts(
            contacts,
            contact_groups,
            device_contacts,
        )
        .into_iter()
        // If we are searching for contact group, we don't want to necessairly show
        // all members of given group
        .filter(|suggestion| {
            suggestion.name.to_lowercase().contains(&query)
                || suggestion
                    .email()
                    .is_some_and(|email| email.to_lowercase().contains(&query))
        })
        .collect();

        Ok(suggestions)
    }

    pub async fn action_delete(
        queue: &Queue,
        contact_ids: Vec<LocalContactId>,
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
        remote_ids: &[ContactId],
        api: &Proton,
    ) -> CoreContextResult<Vec<ContactId>> {
        let response = api
            .put_delete_contacts(remote_ids.iter().cloned().map_into().collect())
            .await?;

        Ok(response
            .responses
            .iter()
            .filter(|r| r.response.code != General::NoError as u32)
            .map(|r| r.id.clone())
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
            remote_id: Some(value.id),
            cards: vec![],
            contact_emails: vec![],
            create_time: value.create_time,
            label_ids: Labels::new(value.label_ids),
            modify_time: value.modify_time,
            name: value.name,
            size: value.size,
            uid: value.uid,
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
            uid: ContactUID::from(String::default()),
            deleted: Default::default(),
            row_id: Default::default(),
        }
    }
}

impl From<ApiContactFull> for Contact {
    fn from(value: ApiContactFull) -> Self {
        Self {
            local_id: None,
            remote_id: Some(value.id),
            cards: value.cards.map_vec(),
            contact_emails: value
                .contact_emails
                .into_iter()
                .map(ContactEmail::from)
                .collect(),
            create_time: value.create_time,
            label_ids: Labels::new(value.label_ids.map_vec()),
            modify_time: value.modify_time,
            name: value.name,
            size: value.size,
            uid: value.uid,
            deleted: false,
            row_id: None,
        }
    }
}

pub struct ContactListWatcher {
    sender: flume::Sender<()>,
}

impl ContactListWatcher {
    /// Creates a new watcher
    #[must_use]
    pub fn new(sender: flume::Sender<()>) -> Self {
        Self { sender }
    }
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
