use crate::utils::MapVec as _;
use std::collections::{BTreeSet, HashMap};
use std::iter;
use std::sync::Arc;
use std::time::Instant;

use crate::actions::contacts::Delete as ContactsDelete;
use crate::datatypes::{
    ContactGroupItem, ContactSuggestions, DeviceContact, GroupedContacts, InitializationKey,
    LabelType, Labels, LocalContactId, LocalLabelId,
};
use crate::models::{ContactCard, ContactEmail, ModelExtension, ModelIdExtension};
use crate::{ContactError, CoreContextError, CoreContextResult};
use anyhow::Context;
use bytes::Buf as _;
use futures::future::try_join;
use futures::try_join;
use ical::VcardParser;
use itertools::Itertools;
use proton_action_queue::queue::{ActionError, Queue, QueuedActionOutput};
use proton_core_api::SYNC_CONTACT_PAGE_SIZE;
use proton_core_api::consts::General;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::ContactId;
use proton_core_api::services::proton::ContactUID;
use proton_core_api::services::proton::ProtonCore;
use proton_core_api::services::proton::{
    ContactBasic as ApiContactBasic, ContactFull as ApiContactFull,
};
use proton_core_api::services::proton::{GetContactsEmailsOptions, GetContactsOptions};
use proton_core_api::session::Session;
use proton_crypto::crypto::PGPProviderSync;
use proton_crypto_account::contacts::{ContactCardType, DecryptableVerifiableCard as _};
use proton_crypto_account::keys::UnlockedUserKeys;
use proton_vcard::vcard::VCard;
use sqlite_watcher::watcher::TableObserver;
use stash::exports::Transaction;
use stash::macros::Model;
use stash::orm::{Model, ModelHooks};
use stash::params;
use stash::stash::{Bond, RunTransaction, Stash, StashError, Tether, WatcherHandle};
use tokio::task::JoinSet;
use tracing::{debug, error, info};

use super::{InitializationError, InitializationWatcher, InitializedComponent, Label};

#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("contacts")]
#[ModelHooks]
pub struct Contact {
    #[IdField(autoincrement)]
    pub local_id: Option<LocalContactId>,

    #[DbField]
    pub remote_id: Option<ContactId>,

    pub cards: Vec<ContactCard>,
    pub contact_emails: Vec<ContactEmail>,

    #[DbField]
    pub create_time: u64,

    #[DbField]
    pub label_ids: Labels,

    #[DbField]
    pub modify_time: u64,

    #[DbField]
    pub name: String,

    #[DbField]
    pub size: u64,

    #[DbField]
    pub uid: ContactUID,

    /// Reflects whether the record has been deleted. This is used to ensure that
    /// delete happens in a two-step process, where the record is marked as
    /// deleted, then deleted from remote, then finally deleted from the local
    /// by event loop update.
    #[DbField]
    pub deleted: bool,
}

impl ModelIdExtension for Contact {
    type RemoteId = ContactId;
    fn remote_id(&self) -> Option<&Self::RemoteId> {
        self.remote_id.as_ref()
    }
}

impl Contact {
    /// Returns the associated cards for a contact.
    ///
    /// This function retrieves the cards for a contact from the database,
    /// stores them in the contact struct, and then returns them.
    ///
    /// # Errors
    ///
    /// Returns a [`StashError`] if the cards cannot be retrieved.
    ///
    pub async fn cards(&mut self, tether: &Tether) -> Result<&[ContactCard], StashError> {
        self.cards = ContactCard::find(
            "WHERE remote_contact_id = ?",
            params![self.remote_id.clone()],
            tether,
        )
        .await?;

        Ok(&self.cards)
    }

    /// Fetches and decrypts all of the vcards for this contact.
    pub async fn vcards<T: PGPProviderSync>(
        &self,
        tether: &Tether,
        provider: &T,
        keys: &UnlockedUserKeys<T>,
    ) -> Result<Vec<VCard>, anyhow::Error> {
        let cards = ContactCard::find(
            "WHERE remote_contact_id = ?",
            params![self.remote_id.clone()],
            tether,
        )
        .await?;

        let mut decrypted_cards = vec![];

        for card in cards {
            let card = match card.decrypt_and_verify_sync(provider, keys, keys) {
                Ok(card) => card,
                Err(e) => {
                    error!("{e:?}");
                    continue;
                }
            };

            for vcard in VcardParser::new(card.reader()) {
                let vcard = vcard?.try_into()?;
                decrypted_cards.push(vcard);
            }
        }

        Ok(decrypted_cards)
    }

    pub async fn vcard_details<T: PGPProviderSync>(
        &self,
        tether: &Tether,
        provider: &T,
        keys: &UnlockedUserKeys<T>,
    ) -> anyhow::Result<VCard> {
        let cards = ContactCard::find(
            "WHERE remote_contact_id = ?",
            params![self.remote_id.clone()],
            tether,
        )
        .await?;

        let card = cards
            .into_iter()
            .find(|c| {
                matches!(
                    c.card_type,
                    ContactCardType::Encrypted | ContactCardType::EncryptedAndSigned
                )
            })
            .context("No card details")?;

        let card = card
            .decrypt_and_verify_sync(provider, keys, keys)
            .context("Error decrypting vCard")?;
        let mut cards = VcardParser::new(card.reader());
        let card = cards
            .next()
            .context("Not vCard in card?")?
            .context("Can't parse vCard with ical")?;
        let card = card
            .try_into()
            .context("Error parsing vCard with proton-vcard")?;
        Ok(card)
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
    pub async fn emails(&mut self, tether: &Tether) -> Result<&[ContactEmail], StashError> {
        self.contact_emails = ContactEmail::find(
            "WHERE remote_contact_id = ? ORDER BY display_order ASC",
            params![self.remote_id.clone()],
            tether,
        )
        .await?;
        Ok(&self.contact_emails)
    }

    /// Updates all user contacts including their emails without their cards.
    ///
    /// The result of this function MUST ONLY be used (as in [`SyncedContacts::store`]) after syncing contact labels.
    ///
    /// # Errors
    ///
    /// Errors when the API request fails or when the database query fails.
    ///
    #[tracing::instrument(skip(api))]
    #[allow(clippy::too_many_lines)]
    pub async fn sync(api: &Session) -> Result<SyncedContacts, ApiServiceError> {
        info!("Syncing contacts");
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
        let contacts: Vec<Contact> = iter::once(Ok(first_contacts.contacts))
            .chain(contacts)
            .flatten()
            .flatten()
            .map(Into::into)
            .collect();
        debug!("Fetched {} contacts", contacts.len());

        let emails = emails_joinset.join_all().await;
        // We don't need the data afterwards so we don't need to Arc it.
        let emails: Vec<ContactEmail> = iter::once(Ok(first_emails.contact_emails))
            .chain(emails)
            .flatten()
            .flatten()
            .map(Into::into)
            .collect();

        debug!("Fetched {} emails", emails.len());
        debug!(
            "Downloaded and converted all contacts in {:?}",
            t0.elapsed()
        );

        // We are splitting the store and download functions in two so that it's faster.
        Ok(SyncedContacts { contacts, emails })
    }

    pub const INIT_KEY: InitializationKey = InitializationKey::new("contacts");

    /// It initializes contats by syncing with the Backend.
    /// In case of successful initialization, it marks it in the [`InitializedComponents`].
    ///
    /// This function is idempotent. If successfully initialized in the past.
    ///
    pub async fn initialize(
        watcher: Arc<InitializationWatcher>,
        api: &Session,
        stash: &Stash,
    ) -> Result<(), InitializationError<CoreContextError>> {
        InitializedComponent::initialize(
            watcher,
            Self::INIT_KEY,
            &[Label::INIT_KEY],
            stash.connection().await?,
            async move || Ok(Self::sync(api).await?),
            async |tx, res| {
                res.store(tx).await?;
                Ok(())
            },
        )
        .await
    }

    /// Updates the full contact with the given ID including its emails and
    /// cards.
    /// Doesn't make an API request if the cards have already been synced.
    /// If you're using this from test code and you're modifying the mocks call
    /// `force_sync_with_card` instead.
    pub async fn sync_with_card(
        local_id: LocalContactId,
        api: &Session,
        tx: &mut impl RunTransaction,
    ) -> CoreContextResult<()> {
        // First let's check if the sync has already happened.
        let c: u32 = tx
            .tether()
            .query_value(
                "SELECT COUNT(*) AS value FROM contact_cards WHERE local_contact_id = ?",
                params![local_id],
            )
            .await?;

        if c != 0 {
            debug!("Contact {local_id} is already synced, skipping fetch");
            return Ok(());
        }

        Self::force_sync_with_card(local_id, api, tx).await
    }

    pub async fn force_sync_with_card(
        local_id: LocalContactId,
        api: &Session,
        tx: &mut impl RunTransaction,
    ) -> CoreContextResult<()> {
        info!("Syncing full contact for contact id {local_id}");
        let remote_id = Contact::local_id_counterpart(local_id, tx.tether())
            .await?
            .ok_or_else(|| {
                CoreContextError::ContactError(ContactError::ContactDoesNotHaveRemoteId(local_id))
            })?;

        let mut contact_with_card = Contact::from(
            api.get_contact(remote_id.clone())
                .await
                .map_err(|err| {
                    error!("Failed to fetch full contact with id {local_id:?}: {err:?}");
                    err
                })?
                .contact,
        );

        tx.run_tx(async |tx| {
            contact_with_card.save(tx).await.map_err(|err| {
                error!("Failed to sync full contact to db: {err:?}");
                err
            })?;

            for email in &mut contact_with_card.contact_emails {
                email.save(tx).await.map_err(|e| {
                    error!("Failed to update contact emails: {e:?}");
                    e
                })?;
            }

            Ok(())
        })
        .await?;
        Ok(())
    }

    /// Returns a list of contacts grouped by the first letter of their name.
    ///
    /// # Errors
    ///
    /// when querying the database fails.
    ///
    #[tracing::instrument(skip_all)]
    pub async fn contact_list(tether: &Tether) -> Result<Vec<GroupedContacts>, StashError> {
        // TODO (ET-2028): Use pagination
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

    // This is not necessary but android wants this.
    //
    // This is particularly inefficient because we're getting all contacts just to get one
    // group, given our data model (labels being serialized into the contacts) we need to do a full scan.
    #[tracing::instrument(skip(tether))]
    pub async fn contact_group_by_id(
        tether: &Tether,
        id: LocalLabelId,
    ) -> Result<ContactGroupItem, StashError> {
        let l = Label::find_by_id(id, tether)
            .await?
            .context("The specified id doesn't exist")?;

        debug_assert_eq!(l.label_type, LabelType::ContactGroup);

        let remote = Label::resolve_remote_label_id(id, tether)
            .await
            .with_context(||
                format!("Local contact groups are not yet implemented: Trying to resolve nonexistent remote label for local label {id}")
            )?;

        let mut res = ContactEmail::load_inner(
            "SELECT contact_emails.* FROM contact_emails
             JOIN contacts ON contact_emails.remote_contact_id = contacts.remote_id
             WHERE contacts.deleted = 0 
             AND EXISTS (
                 SELECT 1 FROM json_each(contacts.label_ids) 
                 WHERE json_each.value = ?
             )
             ORDER BY contact_emails.display_order, contact_emails.local_id",
            params![remote],
            tether,
        )
        .await?;

        res.sort_unstable_by_key(|x| (x.display_order, x.id()));

        Ok(ContactGroupItem {
            local_id: l.id(),
            avatar_information: l.name.as_str().into(),
            name: l.name,
            contacts: res.map_vec(),
        })
    }

    /// Returns a list of contact suggestions (used for example in Composer). Sorted, deduplicated but not filtered by the query.
    ///
    /// # Parameters
    ///
    /// * `device_contacts` - contacts stored in the device storage, not shared between proton clients.
    ///
    /// # Errors
    ///
    /// when querying the database fails.
    ///
    pub async fn contact_suggestions(
        device_contacts: Vec<DeviceContact>,
        tether: &Tether,
    ) -> Result<ContactSuggestions, StashError> {
        // TODO (ET-2028): Use pagination
        let (mut contacts, contact_groups) = try_join!(
            Contact::find("WHERE deleted = 0", vec![], tether),
            Label::find_by_kind(LabelType::ContactGroup, tether)
        )?;

        for contact in &mut contacts {
            contact.emails(tether).await?;
        }

        Ok(ContactSuggestions::from_contacts_and_device_contacts(
            contacts,
            contact_groups,
            device_contacts,
        ))
    }

    pub async fn action_delete(
        queue: &Queue,
        contact_ids: Vec<LocalContactId>,
    ) -> Result<QueuedActionOutput<ContactsDelete>, ActionError<ContactsDelete>> {
        let action = ContactsDelete::new(contact_ids);
        queue.queue_action(action).await
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
        api: &Session,
    ) -> CoreContextResult<Vec<ContactId>> {
        info!("Deleting contacts {remote_ids:?}");
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
        let tether = stash.connection().await?;
        let contacts = Contact::contact_list(&tether).await?;
        let handle = stash.subscribe_to(|sender| Box::new(ContactListWatcher { sender }))?;
        Ok((contacts, handle))
    }
}

impl ModelHooks for Contact {
    fn before_save(&mut self, tx: &Transaction<'_>) -> stash::stash::StashResult<()> {
        if let Some(remote_id) = &self.remote_id {
            if let Some(existing) = Self::find_by_remote_id_sync(remote_id, tx)? {
                self.local_id = existing.local_id;
            }
        } else if let Some(local_id) = self.local_id {
            if let Some(existing) = Self::load_by_id_sync(local_id, tx)? {
                self.remote_id = existing.remote_id;
            }
        }

        Ok(())
    }
    fn after_save(&mut self, tx: &Transaction<'_>) -> Result<(), StashError> {
        for card in &mut self.cards {
            card.local_contact_id = self.local_id;
            card.remote_contact_id.clone_from(&self.remote_id);
        }
        for email in &mut self.contact_emails {
            email.local_contact_id = self.local_id;
            email.remote_contact_id.clone_from(&self.remote_id);
        }
        tx.execute(
            "DELETE FROM contact_cards WHERE local_contact_id = ?",
            (self.local_id,),
        )?;
        for card in &mut self.cards {
            card.local_id = None;
            card.save_sync(tx).map_err(|e| {
                error!("Failed to update contact cards: {e:?}");
                e
            })?;
        }
        Ok(())
    }
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
        }
    }
}

impl Contact {
    #[cfg(feature = "test-utils")]
    #[allow(clippy::default_trait_access)]
    #[must_use]
    pub fn test_default() -> Self {
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
        }
    }
}

pub struct ContactListWatcher {
    sender: flume::Sender<()>,
}

impl ContactListWatcher {
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
            .inspect_err(|e| error!("Failed to send notification for ContactListWatcher: {e:?}"))
            .ok();
    }
}

#[must_use]
#[derive(Debug)]
pub struct SyncedContacts {
    contacts: Vec<Contact>,
    emails: Vec<ContactEmail>,
}

impl SyncedContacts {
    #[tracing::instrument(skip_all)]
    pub async fn store(self, tx: &Bond<'_>) -> Result<(), StashError> {
        let Self {
            contacts,
            mut emails,
        } = self;
        // Let's start with a clean database
        tx.execute("DELETE FROM contacts", vec![]).await?;
        tx.execute("DELETE FROM contact_emails", vec![]).await?;
        tx.execute("DELETE FROM contact_cards", vec![]).await?;
        tx.execute("DELETE FROM contact_email_labels", vec![])
            .await?;

        // We will use this to map the contact_emails to the contacts without having to
        // query the db each time we instert one.
        // We require this to happen since the contact_emails need the local id of its contact.
        let mut id_map = HashMap::new();

        let t1 = Instant::now();
        for mut cont in contacts {
            cont.save(tx).await?;
            id_map.insert(cont.remote_id.clone().unwrap(), cont.id());
        }
        debug!(
            "Stored {} contacts to the db in {:?}",
            id_map.len(),
            t1.elapsed()
        );

        emails.retain_mut(|em| {
            let Some(contact_id) = &em.remote_contact_id else {
                error!("a contact_email has no contact");
                return false;
            };
            let Some(local_id) = id_map.get(contact_id) else {
                error!("a contact_email has no saved local contact");
                return false;
            };
            em.local_contact_id = Some(*local_id);
            true
        });

        let t2 = Instant::now();
        let count = emails.len();
        for mut em in emails {
            em.save(tx).await?;
        }

        debug!(
            "Stored {count} contacts_emails to the db in {:?}",
            t2.elapsed()
        );

        debug!("Stored all to the db in {:?}", t1.elapsed());
        Ok(())
    }
}
