use crate::utils::{MapVec as _, Paginatable};
use std::collections::{BTreeSet, HashMap};
use std::default::Default;
use std::io::Cursor;
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;

use super::{InitializationError, InitializationWatcher, InitializedComponent, Label};
use crate::actions::contacts::Delete as ContactsDelete;
use crate::datatypes::{
    ContactGroupItem, ContactSuggestions, DeviceContact, GroupedContacts, InitializationKey,
    LabelType, Labels, LocalContactId, LocalLabelId,
};
use crate::event_loop::events::Action;
use crate::models::{ContactCard, ContactEmail, ModelExtension, ModelIdExtension};
use crate::{ContactError, CoreContextError, CoreContextResult};
use anyhow::Context;
use bytes::Buf as _;
use futures::future::try_join_all;
use futures::try_join;
use ical::VcardParser;
use itertools::Itertools;
use proton_action_queue::queue::{ActionError, Queue, QueuedActionOutput};
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::SYNC_CONTACT_PAGE_SIZE;
use proton_core_api::consts::General;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::ProtonCore;
use proton_core_api::services::proton::{
    ContactBasic as ApiContactBasic, ContactEmail as ApiContactEmail, ContactFull as ApiContactFull,
};
use proton_core_api::services::proton::{ContactId, GetContactsResponse};
use proton_core_api::services::proton::{ContactUID, GetContactsEmailsResponse};
use proton_core_api::services::proton::{GetContactsEmailsOptions, GetContactsOptions};
use proton_core_api::session::Session;
use proton_crypto::crypto::PGPProviderSync;
use proton_crypto_account::contacts::DecryptableVerifiableCard as _;
use proton_crypto_account::keys::UnlockedUserKeys;
use proton_vcard::vcard::{PropertyUid, VCard};
use sqlite_watcher::watcher::TableObserver;
use stash::exports::Transaction;
use stash::macros::Model;
use stash::orm::{DbRecord, Model, ModelHooks};
use stash::params;
use stash::rusqlite::params_from_iter;
use stash::stash::{Bond, RunTransaction, Stash, StashError, Tether, WatcherHandle};
use tracing::{debug, error, info};

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

#[derive(Debug, Error)]
#[error("Cannot merge vCards: duplicate property found")]
pub struct DuplicatedVCardProperty;

impl Contact {
    /// Returns the associated cards for a contact.
    ///
    /// This function retrieves the cards for a contact from the database,
    /// stores them in the contact struct, and then returns them.
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

        let blobs = cards
            .into_iter()
            .map(|contact_card| {
                contact_card
                    .decrypt_and_verify_sync(provider, keys, keys)
                    .context("Error decrypting vCard")
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        Self::merged_vcards_from_decrypted_blobs(blobs)
    }

    /// Parses one decrypted blob (which may contain multiple vCards) into `Vec<VCard>`
    fn vcards_from_bytes(bytes: &[u8]) -> anyhow::Result<Vec<VCard>> {
        let text = String::from_utf8_lossy(bytes).into_owned();
        let mut vcards = Vec::new();
        let parser = VcardParser::new(Cursor::new(text.as_bytes()));

        for card_res in parser {
            let vcard_contact = card_res.context("Can't parse vCard with ical")?;
            let vcard: VCard = vcard_contact
                .try_into()
                .context("Error parsing vCard with proton-vcard")?;
            vcards.push(vcard);
        }
        Ok(vcards)
    }

    /// Merges one or more decrypted vCard blobs into a single `VCard`
    fn merged_vcards_from_decrypted_blobs(blobs: Vec<Vec<u8>>) -> anyhow::Result<VCard> {
        let mut merged: Option<VCard> = None;

        for bytes in blobs {
            let vcards = Self::vcards_from_bytes(&bytes)?;
            for vcard in vcards {
                merged = Some(match merged {
                    Some(acc) => Self::merged_disjoint(acc, vcard)?,
                    None => vcard,
                });
            }
        }

        merged.context("No VCARD data in provided blobs")
    }

    fn merged_disjoint(lhs: VCard, rhs: VCard) -> Result<VCard, DuplicatedVCardProperty> {
        fn pick<Property>(
            lhs: HashMap<PropertyUid, Property>,
            rhs: HashMap<PropertyUid, Property>,
        ) -> Result<HashMap<PropertyUid, Property>, DuplicatedVCardProperty> {
            match (lhs.is_empty(), rhs.is_empty()) {
                (true, true) => Ok(HashMap::new()),
                (true, false) => Ok(rhs),
                (false, true) => Ok(lhs),
                (false, false) => Err(DuplicatedVCardProperty),
            }
        }

        let mut merged = VCard::default();

        merged.addresses = pick(lhs.addresses, rhs.addresses)?;
        merged.calendar_addresses = pick(lhs.calendar_addresses, rhs.calendar_addresses)?;
        merged.calendar_user_addresses =
            pick(lhs.calendar_user_addresses, rhs.calendar_user_addresses)?;
        merged.categories = pick(lhs.categories, rhs.categories)?;
        merged.client_pid_map = pick(lhs.client_pid_map, rhs.client_pid_map)?;
        merged.emails = pick(lhs.emails, rhs.emails)?;
        merged.fburls = pick(lhs.fburls, rhs.fburls)?;
        merged.formatted_names = pick(lhs.formatted_names, rhs.formatted_names)?;
        merged.geos = pick(lhs.geos, rhs.geos)?;
        merged.impps = pick(lhs.impps, rhs.impps)?;
        merged.keys = pick(lhs.keys, rhs.keys)?;
        merged.languages = pick(lhs.languages, rhs.languages)?;
        merged.logos = pick(lhs.logos, rhs.logos)?;
        merged.members = pick(lhs.members, rhs.members)?;
        merged.nicknames = pick(lhs.nicknames, rhs.nicknames)?;
        merged.notes = pick(lhs.notes, rhs.notes)?;
        merged.organizations = pick(lhs.organizations, rhs.organizations)?;
        merged.photos = pick(lhs.photos, rhs.photos)?;
        merged.related = pick(lhs.related, rhs.related)?;
        merged.roles = pick(lhs.roles, rhs.roles)?;
        merged.sounds = pick(lhs.sounds, rhs.sounds)?;
        merged.sources = pick(lhs.sources, rhs.sources)?;
        merged.telephones = pick(lhs.telephones, rhs.telephones)?;
        merged.time_zones = pick(lhs.time_zones, rhs.time_zones)?;
        merged.titles = pick(lhs.titles, rhs.titles)?;
        merged.urls = pick(lhs.urls, rhs.urls)?;
        merged.xmls = pick(lhs.xmls, rhs.xmls)?;
        merged.xtendeds = pick(lhs.xtendeds, rhs.xtendeds)?;

        merged.anniversary = lhs.anniversary.or(rhs.anniversary);
        merged.birthday = lhs.birthday.or(rhs.birthday);
        merged.gender = lhs.gender.or(rhs.gender);
        merged.kind = lhs.kind.or(rhs.kind);
        merged.name = lhs.name.or(rhs.name);
        merged.product_id = lhs.product_id.or(rhs.product_id);
        merged.revision = lhs.revision.or(rhs.revision);
        merged.uid = lhs.uid.or(rhs.uid);

        Ok(merged)
    }

    /// Returns the associated emails for a contact.
    ///
    /// This function retrieves the emails for a contact from the database,
    /// stores them in the contact struct, and then returns them.
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
    #[tracing::instrument(skip(api))]
    #[allow(clippy::too_many_lines)]
    pub async fn sync(api: &Session) -> Result<SyncedContacts, ApiServiceError> {
        info!("Syncing contacts");

        let contacts = PaginateContacts::fetch_all(api);
        let emails = PaginateEmails::fetch_all(api);

        let (contacts, emails) = tokio::try_join!(contacts, emails)?;
        let contacts = contacts.into_iter().map(Into::into).collect();
        let emails = emails.into_iter().map(Into::into).collect();

        // We are splitting the store and download functions in two so that it's faster.
        Ok(SyncedContacts { contacts, emails })
    }

    pub const INIT_KEY: InitializationKey = InitializationKey::new("contacts");

    /// It initializes contacts by syncing with the Backend.
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
            |tx, res| {
                res.store(tx)?;
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
                "SELECT COUNT(*) FROM contact_cards WHERE local_contact_id = ?",
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

    pub async fn sync_contacts_by_ids(
        api: &Session,
        contact_ids: Vec<ContactId>,
        tx: &mut impl RunTransaction,
    ) -> Result<Vec<Self>, ApiServiceError> {
        let batch_size = 10;
        let mut contacts: Vec<Self> = Vec::new();

        for batch in contact_ids.chunks(batch_size) {
            let requests = batch.iter().map(|id| api.get_contact(id.clone()));
            let responses = try_join_all(requests).await?;

            contacts.extend(
                responses
                    .into_iter()
                    .map(|response| response.contact.into()),
            );
        }

        tx.run_tx(async |tx| {
            for contact in &mut contacts {
                contact.save(tx).await?;
            }
            Ok(())
        })
        .await?;

        Ok(contacts)
    }

    /// Returns a list of contacts grouped by the first letter of their name.
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
    /// which can revert the deletion of a contact in case of something unpredictable happened.
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
        let handle = stash
            .subscribe_to(|sender| Box::new(ContactListWatcher { sender }))
            .await?;
        Ok((contacts, handle))
    }

    pub async fn handle_event(
        tx: &Bond<'_>,
        id: &ContactId,
        action: Action,
        contact: Option<&mut Contact>,
        changeset: &mut RebaseChangeSet,
    ) -> Result<(), StashError> {
        action
            .log_entry(id, async |remote_id| {
                Contact::remote_id_counterpart(remote_id.clone(), tx)
                    .await
                    .unwrap_or_default()
                    .map(|v| v.as_u64())
            })
            .await;

        match action {
            Action::Delete => tx
                .execute(
                    "DELETE FROM contacts WHERE remote_id = ?",
                    params![id.clone()],
                )
                .await
                .map(|_| ())
                .map_err(|e| {
                    error!("Failed to delete contact: {e:?}");
                    e
                })?,
            Action::Create | Action::Update => {
                if let Some(contact) = contact {
                    contact.save(tx).await.map_err(|e| {
                        error!("Failed to create or update contact: {e:?}");
                        e
                    })?;
                    changeset.add(contact.id());
                }
            }
            Action::UpdateFlags => (),
        }
        Ok(())
    }
}

impl ModelHooks for Contact {
    fn before_save(&mut self, tx: &Transaction<'_>) -> stash::stash::StashResult<()> {
        // WARN: For performance reasons this will NOT be called in the initial sync. See `SyncedContacts::store`
        // Any extra logic here should be copied there.
        if let Some(remote_id) = &self.remote_id {
            if let Some(existing) = Self::find_by_remote_id_sync(remote_id, tx)? {
                self.local_id = existing.local_id;
            }
        } else if let Some(local_id) = self.local_id
            && let Some(existing) = Self::load_by_id_sync(local_id, tx)?
        {
            self.remote_id = existing.remote_id;
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
    pub fn store(self, tx: &Transaction<'_>) -> Result<(), StashError> {
        let Self {
            contacts,
            mut emails,
        } = self;
        // Let's start with a clean database
        tx.execute_batch(
            "
        DELETE FROM contacts;
        DELETE FROM contact_emails;
        DELETE FROM contact_cards;
        DELETE FROM contact_email_labels;",
        )?;

        // We will use this to map the contact_emails to the contacts without having to
        // query the db each time we insert one.
        // We require this to happen since the contact_emails need the local id of its contact.
        let mut id_map = HashMap::new();

        let t1 = Instant::now();
        let mut q = tx.prepare(Contact::INSERT_QUERY)?;
        for cont in contacts {
            let params = params_from_iter(cont.field_values());
            let id = q.query_row(params, |r| r.get(0))?;
            id_map.insert(cont.remote_id.clone().unwrap(), id);
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
        let mut q = tx.prepare(ContactEmail::INSERT_QUERY)?;
        let count = emails.len();
        for em in emails {
            let params = params_from_iter(em.field_values());
            q.query(params)?.next()?;
        }

        debug!(
            "Stored {count} contacts_emails to the db in {:?}",
            t2.elapsed()
        );

        debug!("Stored all to the db in {:?}", t1.elapsed());
        Ok(())
    }
}

struct PaginateContacts;
impl Paginatable for PaginateContacts {
    type PaginateOptions = GetContactsOptions;

    type Response = GetContactsResponse;

    type Output = ApiContactBasic;

    type Error = ApiServiceError;

    type API = Session;

    const NAME: &'static str = "Contacts";

    const DEFAULT_PAGE_SIZE: u64 = SYNC_CONTACT_PAGE_SIZE;

    async fn fetch(
        api: &Self::API,
        options: Self::PaginateOptions,
    ) -> Result<Self::Response, Self::Error> {
        api.get_contacts(options).await
    }
}

struct PaginateEmails;
impl Paginatable for PaginateEmails {
    type PaginateOptions = GetContactsEmailsOptions;

    type Response = GetContactsEmailsResponse;

    type Output = ApiContactEmail;

    type Error = ApiServiceError;

    type API = Session;

    const NAME: &'static str = "Emails";

    const DEFAULT_PAGE_SIZE: u64 = SYNC_CONTACT_PAGE_SIZE;

    async fn fetch(
        api: &Self::API,
        options: Self::PaginateOptions,
    ) -> Result<Self::Response, Self::Error> {
        api.get_contacts_emails(options).await
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merged_vcards_from_decrypted_blobs_with_duplicated_properties_throws_error() {
        let blob1 = "
BEGIN:VCARD
VERSION:4.0
PRODID;TYPE=text;VALUE=TEXT:pm-ez-vcard 0.0.1
ITEM1.EMAIL:one@passmail.net
END:VCARD"
            .trim()
            .replace('\n', "\r\n")
            .into_bytes();

        let blob2 = "
BEGIN:VCARD
VERSION:4.0
PRODID:-//ProtonMail//ProtonMail vCard 1.0.0//EN
ITEM2.EMAIL:two@passmail.net
END:VCARD"
            .trim()
            .replace('\n', "\r\n")
            .into_bytes();

        let given = Contact::merged_vcards_from_decrypted_blobs(vec![blob1, blob2]);

        assert!(
            given
                .as_ref()
                .expect_err("Expected error")
                .downcast_ref::<DuplicatedVCardProperty>()
                .is_some(),
            "Expected DuplicatedVCardProperty error"
        );
    }

    #[test]
    fn merged_vcards_from_decrypted_blobs_merges_three_split_blobs() {
        let blob1 = "BEGIN:VCARD
VERSION:4.0
N:;11111111123232323;;;
TEL;PREF=1:2345678
ADR;PREF=1:;;vb ;n;m;n;nj
ADR;PREF=2:;;jk;j;j;jm;k
NOTE:fgchvbjnkm\\nfcgvhbjnkml\\n\\\\ghjknml\\nvhbkm\\nnbm\\nnbm\\,\\.\\nbnmkl\\,\\n mn\\,\\nnbm\\,\\n\\\\
END:VCARD"
            .replace('\n', "\r\n")
            .into_bytes();

        let blob2 = "BEGIN:VCARD
VERSION:4.0
PRODID;TYPE=text;VALUE=TEXT:pm-ez-vcard 0.0.1
UID:protonmail-ios-autoimport-E233D520-6965-4442-8C54-8F627E77399C
FN;PREF=1:11111111123232323
ITEM1.EMAIL;PREF=1:fkjhkdfgjhdghjdgkjhdgkfjhdjkhkdjfhg@pm.me
ITEM2.EMAIL;PREF=2:proton.domelike477@passmail.net
ITEM3.EMAIL;PREF=3:proton.rectangle212@passmail.net
ITEM4.EMAIL;PREF=4:proton.splotchy980@passmail.net
END:VCARD"
            .replace('\n', "\r\n")
            .into_bytes();

        let blob3 = "BEGIN:VCARD
VERSION:4.0
PRODID:-//ProtonMail//ProtonMail vCard 1.0.0//EN
ITEM1.CATEGORIES:New test group #1 [Mateusz],New test group #2 [Mateusz]
END:VCARD"
            .replace('\n', "\r\n")
            .into_bytes();

        let given = Contact::merged_vcards_from_decrypted_blobs(vec![blob1, blob2, blob3])
            .expect("should merge into a single VCard");

        insta::assert_snapshot!(given);
    }
}
