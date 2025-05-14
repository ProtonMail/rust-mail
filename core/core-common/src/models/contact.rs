use crate::utils::MapVec as _;
use std::collections::{BTreeSet, HashMap};
use std::fmt::{self, Display};
use std::iter;
use std::sync::Arc;
use std::time::Instant;

use crate::actions::contacts::Delete as ContactsDelete;
use crate::datatypes::{
    ContactItem, ContactSuggestions, DeviceContact, GroupedContacts, InitializationKey, LabelType,
    Labels, LocalContactId,
};
use crate::models::{ContactCard, ContactEmail, ModelExtension, ModelIdExtension};
use crate::{ContactError, CoreContextError, CoreContextResult, UserContext};
use anyhow::Context;
use bytes::Buf as _;
use futures::future::try_join;
use futures::try_join;
use ical::VcardParser;
use itertools::Itertools;
use proton_action_queue::queue::{ActionError, Queue, QueuedActionOutput};
use proton_core_api::SYNC_CONTACT_PAGE_SIZE;
use proton_core_api::consts::General;
use proton_core_api::services::proton::ContactId;
use proton_core_api::services::proton::ContactUID;
use proton_core_api::services::proton::{
    ContactBasic as ApiContactBasic, ContactFull as ApiContactFull,
};
use proton_core_api::services::proton::{GetContactsEmailsOptions, GetContactsOptions};
use proton_core_api::services::proton::{Proton, ProtonCore};
use proton_crypto::crypto::PGPProviderSync;
use proton_crypto::new_pgp_provider;
use proton_crypto_account::contacts::DecryptableVerifiableCard as _;
use proton_crypto_account::keys::UnlockedUserKeys;
use proton_vcard::address::Address as VcardAddress;
use proton_vcard::gender::GenderValue;
use proton_vcard::parameters::type_generic::GenericType;
use proton_vcard::parameters::type_tel::TelType;
use proton_vcard::values::date_and_or_time::MaybeDateAndOrTime;
use proton_vcard::vcard::{ToSorted, VCard};
use sqlite_watcher::watcher::TableObserver;
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, RunTransaction, Stash, StashError, Tether, WatcherHandle};
use tokio::task::JoinSet;
use tracing::{debug, error};

use super::{InitializationError, InitializationWatcher, InitializedComponent, Label};

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
    ///   use for finding the records.
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
                error!("Failed to update contact cards: {e:?}");
                e
            })?;
        }
        Ok(())
    }

    /// Updates all user contacts including their emails without their cards.
    ///
    /// The result of this function MUST ONLY be used (as in [`SyncedContacts::store`]) after syncing contact labels.
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
    #[tracing::instrument(skip(api))]
    #[allow(clippy::too_many_lines)]
    #[must_use]
    pub async fn sync(api: &Proton) -> CoreContextResult<SyncedContacts> {
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

        let emails = emails_joinset.join_all().await;
        // We don't need the data afterwards so we don't need to Arc it.
        let emails: Vec<ContactEmail> = iter::once(Ok(first_emails.contact_emails))
            .chain(emails)
            .flatten()
            .flatten()
            .map(Into::into)
            .collect();

        debug!(
            "Downloaded and converted all contacts in {:?}",
            t0.elapsed()
        );

        // We are splitting the store and download functions in two so that it's faster.
        Ok(SyncedContacts {
            contacts,
            emails,
            t0,
        })
    }

    /// Key used to distinguish between components in the initialization.
    /// It is a string, not an enum for making it open for additional changes from different BU.
    ///
    pub const INIT_KEY: InitializationKey = InitializationKey::new("contacts");
    /// It initializes contats by syncing with the Backend.
    /// In case of successful initialization, it marks it in the [`InitializedComponents`].
    ///
    /// This function is idempotent. If successfully initialized in the past.
    ///
    pub async fn initialize(
        watcher: Arc<InitializationWatcher>,
        api: &Proton,
        stash: &Stash,
    ) -> Result<(), InitializationError<CoreContextError>> {
        InitializedComponent::initialize::<CoreContextError, SyncedContacts>(
            watcher,
            Self::INIT_KEY,
            &[Label::INIT_KEY],
            stash.connection(),
            async move || Self::sync(api).await,
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
        api: &Proton,
        rt: &mut impl RunTransaction,
    ) -> CoreContextResult<()> {
        // First let's check if the sync has already happened.
        let c: u32 = rt
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

        Self::force_sync_with_card(local_id, api, rt).await
    }

    pub async fn force_sync_with_card(
        local_id: LocalContactId,
        api: &Proton,
        rt: &mut impl RunTransaction,
    ) -> CoreContextResult<()> {
        debug!("Syncing full contact for contact id {local_id}");
        let remote_id = Contact::local_id_counterpart(local_id, rt.tether())
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

        rt.run_tx(async |tx| {
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
    /// # Parameters
    ///
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///
    /// # Errors
    ///
    /// when querying the database fails.
    ///
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

    /// Returns a list of contact suggestions (used for example in Composer). Sorted, deduplicated but not filtered by the query.
    ///
    /// # Parameters
    ///
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
            .inspect_err(|e| error!("Failed to send notification for ContactListWatcher: {e:?}"))
            .ok();
    }
}

/// This is a manual implementation of `Contact::sync` async closure.
///
/// We keep it as it is until Rust allows us to use `impl Trait` in generics etc.
#[must_use]
#[derive(Debug)]
pub struct SyncedContacts {
    contacts: Vec<Contact>,
    emails: Vec<ContactEmail>,
    t0: Instant,
}

impl SyncedContacts {
    /// Consume this manual closure by storing data in the Database.
    /// Attention: This function should be executed only after Labels are synchronized
    ///
    /// # Panics
    ///
    /// Panics if the local id does exist
    ///
    #[tracing::instrument(skip_all)]
    pub async fn store(self, tx: &Bond<'_>) -> CoreContextResult<()> {
        let Self {
            contacts,
            mut emails,
            t0,
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
            id_map.insert(cont.remote_id.clone().unwrap(), cont.local_id.unwrap());
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
        debug!("Synced all contacts in {:?}", t0.elapsed());
        Ok(())
    }
}

impl ContactDetailCard {
    /// Transforms the data in the vCard struct to something suitable for human consumption
    fn from_vcard(vcard: VCard) -> Self {
        let phones = vcard.telephones.to_sorted(|tel| Telephone {
            number: tel.value.to_string(),
            tel_types: tel.tel_type.iter().cloned().map_vec(),
        });

        let address = vcard.addresses.to_sorted(ContactDetailAddress::from);

        let extended_name = vcard.name.map(|name| ExtendedName {
            last: name.last.concat_to_string(" "),
            first: name.first.concat_to_string(" "),
            additional: name.additional.concat_to_string(" "),
            prefix: name.prefix.concat_to_string(" "),
            suffix: name.suffix.concat_to_string(" "),
        });

        let urls = vcard.urls.to_sorted(|u| VCardUrl {
            url_type: u.r#type.into_iter().map_vec(),
            url: u.value,
        });

        let organizations = vcard
            .organizations
            .to_sorted(|x| x.values.into_iter().join(", "));

        let logos = vcard.logos.to_sorted(|logo| logo.value.0.to_string());
        let photos = vcard.photos.to_sorted(|photo| photo.value.0.to_string());
        let timezones = vcard.time_zones.to_sorted(|x| x.value.to_string());
        let notes = vcard.notes.to_sorted(|x| x.value.value);
        let gender = vcard.gender.map(|g| g.value.into());
        let titles = vcard.titles.to_sorted(|x| x.value.value);
        let roles = vcard.roles.to_sorted(|x| x.value.value);
        let languages = vcard.languages.to_sorted(|x| x.value);
        let members = vcard.members.to_sorted(|x| x.value);
        let anniversary = vcard.anniversary.map(|a| a.value);
        let birthday = vcard.birthday.map(|a| a.value);

        ContactDetailCard {
            extended_name,
            address,
            phones,
            birthday,
            notes,
            anniversary,
            urls,
            gender,
            photos,
            logos,
            titles,
            roles,
            languages,
            timezones,
            members,
            organizations,
        }
    }
}

pub struct ContactDetails {
    pub item: ContactItem,
    pub cards: Vec<ContactDetailCard>,
}

/// Represents some data known from the vCard
#[derive(Default, Clone, Debug)]
pub struct ContactDetailCard {
    pub extended_name: Option<ExtendedName>,
    pub address: Vec<ContactDetailAddress>,
    pub phones: Vec<Telephone>,
    pub birthday: Option<MaybeDateAndOrTime>,
    pub notes: Vec<String>,

    pub anniversary: Option<MaybeDateAndOrTime>,
    pub urls: Vec<VCardUrl>,
    pub gender: Option<GenderType>,
    pub photos: Vec<String>,
    /// Normally a valid link, but needs not be.
    pub logos: Vec<String>,
    pub titles: Vec<String>,
    pub roles: Vec<String>,
    /// This might be an RFC compliant string like es-ES or not, like Spanish or Español
    pub languages: Vec<String>,
    pub timezones: Vec<String>,
    /// Normally a valid link, but needs not be.
    pub members: Vec<String>,
    pub organizations: Vec<String>,
}

impl ContactDetails {
    pub async fn get_from_contact(
        ctx: &UserContext,
        contact_id: LocalContactId,
    ) -> anyhow::Result<Self> {
        let mut tether = ctx.stash().connection();
        Contact::sync_with_card(contact_id, ctx.session(), &mut tether).await?;
        let contact = Contact::load(contact_id, &tether)
            .await?
            .context("Contact does not exist")?;

        let pgp_provider = new_pgp_provider();
        let unlocked_user_keys = ctx
            .unlocked_user_keys(&pgp_provider, &tether, ctx.session())
            .await?;

        let cards = contact
            .vcards(&tether, &pgp_provider, &unlocked_user_keys)
            .await?;

        Ok(Self {
            item: contact.into(),
            cards: cards
                .into_iter()
                .map(ContactDetailCard::from_vcard)
                .collect(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct ExtendedName {
    pub last: Option<String>,
    pub first: Option<String>,
    /// additional names
    pub additional: Option<String>,
    /// honorific prefix
    pub prefix: Option<String>,
    /// honorific suffix
    pub suffix: Option<String>,
}

#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub struct ContactDetailAddress {
    pub street: String,
    pub city: String,
    pub region: String,
    pub postal_code: String,
    pub country: String,
    pub addr_type: Vec<VcardPropType>,
}

impl From<VcardAddress> for ContactDetailAddress {
    fn from(value: VcardAddress) -> Self {
        Self {
            street: value.street,
            city: value.locality,
            region: value.region,
            postal_code: value.postal_code,
            country: value.country,
            addr_type: value.r#type.map_vec(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub struct Telephone {
    pub number: String,
    pub tel_types: Vec<VcardPropType>,
}

#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub struct VCardUrl {
    pub url: String,
    pub url_type: Vec<VcardPropType>,
}

#[derive(Clone, Debug)]
pub struct ContactDetailsEmail {
    pub name: String,
    pub email: String,
}

#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub enum VcardPropType {
    Home,
    Work,
    Text,
    Voice,
    Fax,
    Cell,
    Video,
    Pager,
    TextPhone,
    String(String),
}

impl Display for VcardPropType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VcardPropType::Home => write!(f, "home"),
            VcardPropType::Work => write!(f, "work"),
            VcardPropType::Text => write!(f, "text"),
            VcardPropType::Voice => write!(f, "voice"),
            VcardPropType::Fax => write!(f, "fax"),
            VcardPropType::Cell => write!(f, "cell"),
            VcardPropType::Video => write!(f, "video"),
            VcardPropType::Pager => write!(f, "pager"),
            VcardPropType::TextPhone => write!(f, "textphone"),
            VcardPropType::String(s) => write!(f, "{s}"),
        }
    }
}

impl From<GenericType> for VcardPropType {
    fn from(value: GenericType) -> Self {
        match value {
            GenericType::Home => VcardPropType::Home,
            GenericType::Work => VcardPropType::Work,
            GenericType::IanaToken(tok) => VcardPropType::String(tok.0),
            GenericType::XName(xname) => VcardPropType::String(xname.0),
        }
    }
}

impl From<TelType> for VcardPropType {
    fn from(value: TelType) -> Self {
        match value {
            TelType::Home => VcardPropType::Home,
            TelType::Work => VcardPropType::Work,
            TelType::Text => VcardPropType::Text,
            TelType::Voice => VcardPropType::Voice,
            TelType::Fax => VcardPropType::Fax,
            TelType::Cell => VcardPropType::Cell,
            TelType::Video => VcardPropType::Video,
            TelType::Pager => VcardPropType::Pager,
            TelType::TextPhone => VcardPropType::TextPhone,
            TelType::IanaToken(tok) => VcardPropType::String(tok.0),
            TelType::XName(xname) => VcardPropType::String(xname.0),
        }
    }
}

#[derive(Clone, Debug)]
pub enum GenderType {
    Male,
    Female,
    Other,
    NotApplicable,
    Unknown,
    None,
    /// Other, non standard gender. This could be a user writing "male", "woman", "spaghetti", etc.
    /// NB in proton this is used for the vCards.
    String(String),
}

impl Display for GenderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GenderType::Male => write!(f, "male"),
            GenderType::Female => write!(f, "female"),
            GenderType::Other => write!(f, "other"),
            GenderType::NotApplicable => write!(f, "N/A"),
            GenderType::Unknown => write!(f, "unknown"),
            GenderType::None => write!(f, "none"),
            GenderType::String(value) => write!(f, "{value}"),
        }
    }
}

impl From<GenderValue> for GenderType {
    fn from(value: GenderValue) -> Self {
        match value {
            GenderValue::Male(_) => GenderType::Male,
            GenderValue::Female(_) => GenderType::Female,
            GenderValue::Other(_) => GenderType::Other,
            GenderValue::NotApplicable(_) => GenderType::NotApplicable,
            GenderValue::Unknown(_) => GenderType::Unknown,
            GenderValue::None(_) => GenderType::None,
            GenderValue::Custom(value) => GenderType::String(value),
        }
    }
}

#[cfg(test)]
mod test {
    use insta::assert_debug_snapshot;
    use proton_vcard::vcard::VCard;

    use super::*;

    #[allow(unused, reason = "The fields are only used for their debug impl")]
    #[derive(Debug)]
    struct Snapshot {
        vcard: &'static str,
        card: ContactDetailCard,
    }

    fn get_vcard(raw_vcard: &'static str) -> Snapshot {
        let mut r = VcardParser::new(raw_vcard.as_bytes().reader());
        let c = r.next().expect("Expected 1 card").unwrap();
        assert!(r.next().is_none(), "Expected exactly 1 card");
        let vcard = VCard::from_ical_contact(c).unwrap();
        Snapshot {
            vcard: raw_vcard,
            card: ContactDetailCard::from_vcard(vcard),
        }
    }

    #[test]
    fn real_contact() {
        let real = include_str!("../../tests/vcards/real.vcf");
        assert_debug_snapshot!(get_vcard(real));
    }
    #[test]
    fn real_autosave() {
        let real_autosave = include_str!("../../tests/vcards/real-autosave.vcf");
        assert_debug_snapshot!(get_vcard(real_autosave));
    }

    #[test]
    fn full() {
        let full = include_str!("../../tests/vcards/full.vcf");
        assert_debug_snapshot!(get_vcard(full));
    }

    #[test]
    fn small() {
        let small = include_str!("../../tests/vcards/small.vcf");
        assert_debug_snapshot!(get_vcard(small));
    }

    #[test]
    fn vcard_v3() {
        let v3 = include_str!("../../tests/vcards/v3.vcf");
        assert_debug_snapshot!(get_vcard(v3));
    }

    #[test]
    fn frodo() {
        let frodo = include_str!("../../tests/vcards/frodo.vcf");
        assert_debug_snapshot!(get_vcard(frodo));
    }
}
