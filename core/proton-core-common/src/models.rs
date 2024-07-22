//! Models for the Proton Core common library.
//!
//! This module contains the models used by the Proton Core common library.
//! Models are data structures that can be saved in the database, and are used
//! to represent usable persistent data throughout the application. They are
//! distinctly different from any comparative structures used when interfacing
//! with the Proton API, which are used to represent data in transit only.
//!
//! Notably, the types in this module need to have [`Model`] applied, as they
//! should represent a record in a database table. All of their fields need to
//! be convertible to and from database-compatible format using [`ToSql`](stash::exports::ToSql)
//! and [`FromSql`](stash::exports::FromSql). They do not generally need to be
//! serializable or deserializable, as they are not used for network
//! communication or any other interchange purpose as a general requirement, and
//! so implementation of [`Serialize`](serde::Serialize) and [`Deserialize`](serde::Deserialize)
//! is not necessary and may be a sign of a mistake. The exception here is for
//! child types, used by the models, for which these [`serde`] conversions are
//! desirable to lean on in order to provide conversion to and from SQL types,
//! for instance using [`sql_using_serde`](stash::utils::sql_using_serde), as a
//! convenience mechanism. This is notably useful when wanting to store types as
//! JSON in a database field, for instance. However, child types should be
//! placed into the [`datatypes`](crate::datatypes) module, with only
//! first-order models being placed into this module.
//!
//! Generally speaking, [`From`] conversions to convert from the Proton API
//! types to the internal types are provided, but not vice versa unless there is
//! a specific need.
//!

use crate::datatypes::{
    AddressKeys, AddressSignedKeyList, AddressStatus, AddressType, CardType,
    ContactSendingPreferences, ContactTypes, DateFormat, Density, Email, Flags, HighSecurity,
    LabelId, Labels, LogAuth, Password, Phone, ProductUsedSpace, Referral, RemoteId, SettingsFlags,
    TimeFormat, TwoFa, UserKeys, UserMnemonicStatus, UserType, WeekStart,
};
use crate::CoreContextResult;
use proton_api_core::services::proton::requests::{GetContactsEmailsOptions, GetContactsOptions};
use proton_api_core::services::proton::response_data::{
    Address as ApiAddress, ContactBasic as ApiContactBasic, ContactCard as ApiContactCard,
    ContactEmail as ApiContactEmail, ContactFull as ApiContactFull, User as ApiUser,
    UserSettings as ApiUserSettings,
};
use proton_api_core::services::proton::Proton;
use proton_api_core::SYNC_CONTACT_PAGE_SIZE;
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::{Stash, StashError, Tether};
use tracing::{debug, error};

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("addresses")]
#[allow(clippy::struct_excessive_bools)]
pub struct Address {
    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    #[IdField(optional)]
    pub remote_id: Option<RemoteId>,

    /// TODO: Document this field.
    #[DbField]
    pub address_type: AddressType,

    /// TODO: Document this field.
    #[DbField]
    pub catch_all: bool,

    /// TODO: Document this field.
    #[DbField]
    pub display_name: String,

    /// TODO: Document this field.
    #[DbField]
    pub display_order: u32,

    /// TODO: Document this field.
    #[DbField]
    pub domain_id: Option<String>,

    /// TODO: Document this field.
    #[DbField]
    pub email: String,

    /// TODO: Document this field.
    #[DbField]
    pub keys: AddressKeys,

    /// TODO: Document this field.
    #[DbField]
    pub proton_mx: bool,

    /// TODO: Document this field.
    #[DbField]
    pub receive: bool,

    /// TODO: Document this field.
    #[DbField]
    pub send: bool,

    /// TODO: Document this field.
    #[DbField]
    pub signature: String,

    /// TODO: Document this field.
    #[DbField]
    pub signed_key_list: AddressSignedKeyList,

    /// TODO: Document this field.
    #[DbField]
    pub status: AddressStatus,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,

    /// The database instance that the record is associated with. This is
    /// present for convenience.
    #[StashField]
    pub stash: Option<Stash>,
}

impl Address {
    /// Download and store user addresses into the database
    ///
    /// # Parameters
    ///
    /// * `api`   - The API instance to use to download the addresses.
    /// * `stash` - The database instance to store the addresses.
    ///
    /// # Errors
    ///
    /// TODO: Document the errors.
    ///
    pub async fn sync(api: &Proton, stash: &Stash) -> CoreContextResult<()> {
        let tx = stash.transaction().await?;
        for mut address in api
            .get_addresses()
            .await?
            .addresses
            .into_iter()
            .map(Address::from)
        {
            address.save_using(&tx).await?;
        }

        Ok(())
    }
}

impl From<ApiAddress> for Address {
    fn from(value: ApiAddress) -> Self {
        Self {
            remote_id: Some(value.id.into()),
            address_type: value.address_type.into(),
            catch_all: value.catch_all,
            display_name: value.display_name,
            display_order: value.order,
            domain_id: value.domain_id,
            email: value.email,
            keys: value.keys.into(),
            proton_mx: value.proton_mx,
            receive: value.receive,
            send: value.send,
            signature: value.signature,
            signed_key_list: value.signed_key_list.into(),
            status: value.status.into(),
            row_id: None,
            stash: None,
        }
    }
}

#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("contacts")]
pub struct Contact {
    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    #[IdField(optional)]
    pub remote_id: Option<RemoteId>,

    /// TODO: Document this field.
    pub cards: Vec<ContactCard>,

    /// TODO: Document this field.
    pub contact_emails: Vec<ContactEmail>,

    /// TODO: Document this field.
    #[DbField]
    pub create_time: u64,

    /// TODO: Document this field.
    #[DbField]
    pub label_ids: Labels,

    /// TODO: Document this field.
    #[DbField]
    pub modify_time: u64,

    /// TODO: Document this field.
    #[DbField]
    pub name: String,

    /// TODO: Document this field.
    #[DbField]
    pub size: u64,

    /// TODO: Document this field.
    #[DbField]
    pub uid: RemoteId,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,

    /// The database instance that the record is associated with. This is
    /// present for convenience.
    #[StashField]
    pub stash: Option<Stash>,
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
    pub async fn cards(&mut self) -> Result<&Vec<ContactCard>, StashError> {
        let Some(stash) = self.stash() else {
            return Err(StashError::NoStashAvailable);
        };
        self.cards = ContactCard::find(
            "WHERE remote_contact_id = ?",
            params![self.remote_id.clone()],
            stash,
            None,
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
    pub async fn emails(&mut self) -> Result<&Vec<ContactEmail>, StashError> {
        let Some(stash) = self.stash() else {
            return Err(StashError::NoStashAvailable);
        };
        self.contact_emails = ContactEmail::find(
            "WHERE remote_contact_id = ?",
            params![self.remote_id.clone()],
            stash,
            None,
        )
        .await?;
        Ok(&self.contact_emails)
    }

    /// Overrides [`Model::save()`] to set the contact id for children.
    ///
    /// # Errors
    ///
    /// See [`Model::save()`].
    ///
    pub async fn save(&mut self) -> Result<(), StashError> {
        Model::save(self).await?;
        for card in &mut self.cards {
            card.remote_contact_id.clone_from(&self.remote_id);
        }
        for email in &mut self.contact_emails {
            email.remote_contact_id.clone_from(&self.remote_id);
        }
        let stash = {
            let Some(stash) = self.stash() else {
                return Err(StashError::NoStashAvailable);
            };
            stash.clone()
        };
        stash
            .execute(
                "DELETE FROM contact_cards WHERE remote_contact_id = ?",
                params![self.remote_id.clone()],
            )
            .await?;
        for card in &mut self.cards {
            card.local_id = None;
            card.row_id = None;
            card.set_stash(&stash);
            card.save().await.map_err(|e| {
                error!("Failed to update contact cards: {e}");
                e
            })?;
        }
        Ok(())
    }

    /// Overrides [`Model::save_using()`] to set the contact id for children.
    ///
    /// # Parameters
    ///
    /// See [`Model::save()`].
    ///
    /// # Errors
    ///
    /// See [`Model::save()`].
    ///
    pub async fn save_using(&mut self, tether: &Tether) -> Result<(), StashError> {
        Model::save_using(self, tether).await?;
        for card in &mut self.cards {
            card.remote_contact_id.clone_from(&self.remote_id);
        }
        for email in &mut self.contact_emails {
            email.remote_contact_id.clone_from(&self.remote_id);
        }
        tether
            .execute(
                "DELETE FROM contact_cards WHERE remote_contact_id = ?",
                params![self.remote_id.clone()],
            )
            .await?;
        for card in &mut self.cards {
            card.local_id = None;
            card.row_id = None;
            card.save_using(tether).await.map_err(|e| {
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
    /// TODO: Document the errors.
    ///
    #[allow(clippy::too_many_lines)]
    pub async fn sync(api: &Proton, stash: &Stash) -> CoreContextResult<()> {
        // TODO: There should be one transaction for the whole sync.
        let mut page_index = 0;
        // Reset the database state by deleting all contacts.
        stash.execute("DELETE FROM contacts", vec![]).await?;
        stash.execute("DELETE FROM contact_emails", vec![]).await?;
        stash.execute("DELETE FROM contact_cards", vec![]).await?;
        stash
            .execute("DELETE FROM contact_email_labels", vec![])
            .await?;
        Ok(()).map_err(|err: StashError| {
            error!("Failed to reset contact tables: {err}");
            err
        })?;
        // First update the partial contacts since email contacts reference them.
        debug!("Syncing partial contacts");
        loop {
            let mut contacts: Vec<Contact> = api
                .get_contacts(GetContactsOptions {
                    label_id: None,
                    page: page_index,
                    page_size: SYNC_CONTACT_PAGE_SIZE,
                })
                .await
                .map_err(|err| {
                    error!("Failed to fetch contacts for page {page_index}: {err}");
                    err
                })?
                .contacts
                .into_iter()
                .map(Contact::from)
                .collect();
            if !contacts.is_empty() {
                for contact in &mut contacts {
                    contact.stash = Some(stash.clone());
                    contact.save().await.map_err(|err: StashError| {
                        error!("Failed to sync contacts for page {page_index} to db: {err}");
                        err
                    })?;
                }
            }
            debug!(
                "Synced page {} of partial contacts, {} contacts fetched",
                page_index,
                contacts.len()
            );
            if contacts.len() < SYNC_CONTACT_PAGE_SIZE {
                break;
            }
            page_index += 1;
        }

        // Then, update the email contacts.
        page_index = 0;
        debug!("Syncing contact emails");
        loop {
            let mut contact_emails: Vec<ContactEmail> = api
                .get_contacts_emails(GetContactsEmailsOptions {
                    email: None, // TODO: This is the existing behaviour, but seems wrong...
                    label_id: None,
                    page: page_index,
                    page_size: SYNC_CONTACT_PAGE_SIZE,
                })
                .await
                .map_err(|err| {
                    error!("Failed to sync contact emails for page {page_index}: {err}");
                    err
                })?
                .contact_emails
                .into_iter()
                .map(ContactEmail::from)
                .collect();
            if !contact_emails.is_empty() {
                for contact_email in &mut contact_emails {
                    contact_email.stash = Some(stash.clone());
                    contact_email.save().await.map_err(|err: StashError| {
                        error!("Failed to sync contact emails for page {page_index} to db: {err}");
                        err
                    })?;
                }
            }
            debug!(
                "Synced page {} of contact emails, {} contact emails fetched",
                page_index,
                contact_emails.len()
            );
            if contact_emails.len() < SYNC_CONTACT_PAGE_SIZE {
                break;
            }
            page_index += 1;
        }
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
    /// TODO: Document the errors.
    ///
    pub async fn sync_with_card(
        id: RemoteId,
        api: &Proton,
        stash: &Stash,
    ) -> CoreContextResult<()> {
        debug!("Syncing full contact for contact id {id}");
        let mut contact_with_card = Contact::from(
            api.get_contact(id.clone().into())
                .await
                .map_err(|err| {
                    error!("Failed to fetch full contact with id {id}: {err}");
                    err
                })?
                .contact,
        );

        if let Some(remote_id) = &contact_with_card.remote_id {
            let existing = Contact::load(remote_id.clone(), stash)
                .await
                .map_err(|err| {
                    error!("Failed to load contact from db: {err}");
                    err
                })?;
            if let Some(existing) = existing {
                contact_with_card.row_id = existing.row_id;
            }
        }
        contact_with_card.set_stash(stash);
        contact_with_card.save().await.map_err(|err| {
            error!("Failed to sync full contact to db: {err}");
            err
        })?;
        for email in &mut contact_with_card.contact_emails {
            if let Some(remote_id) = &email.remote_id {
                let existing =
                    ContactEmail::load(remote_id.clone(), stash)
                        .await
                        .map_err(|err| {
                            error!("Failed to load contact email from db: {err}");
                            err
                        })?;
                if let Some(existing) = existing {
                    email.row_id = existing.row_id;
                }
            }
            email.set_stash(stash);
            email.save().await.map_err(|e| {
                error!("Failed to update contact emails: {e}");
                e
            })?;
        }
        Ok(())
    }
}

impl From<ApiContactBasic> for Contact {
    fn from(value: ApiContactBasic) -> Self {
        Self {
            remote_id: Some(value.id.into()),
            cards: vec![],
            contact_emails: vec![],
            create_time: value.create_time,
            label_ids: Labels::new(value.label_ids.into_iter().map(LabelId::from).collect()),
            modify_time: value.modify_time,
            name: value.name,
            size: value.size,
            uid: value.uid.into(),
            row_id: None,
            stash: None,
        }
    }
}

impl From<ApiContactFull> for Contact {
    fn from(value: ApiContactFull) -> Self {
        Self {
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
            row_id: None,
            stash: None,
        }
    }
}

/// Represents a contact card.
///
/// Contact cards contain information encoded as a v-card. Cards can be
/// encrypted or signed with the user keys.
///
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("contact_cards")]
pub struct ContactCard {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    #[IdField(autoincrement)]
    pub local_id: Option<u64>,

    /// TODO: Document this field.
    #[DbField]
    pub remote_contact_id: Option<RemoteId>,

    /// TODO: Document this field.
    #[DbField]
    pub card_type: CardType,

    /// TODO: Document this field.
    #[DbField]
    pub data: String,

    /// TODO: Document this field.
    #[DbField]
    pub signature: Option<String>,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,

    /// The database instance that the record is associated with. This is
    /// present for convenience.
    #[StashField]
    pub stash: Option<Stash>,
}

impl From<ApiContactCard> for ContactCard {
    fn from(value: ApiContactCard) -> Self {
        Self {
            local_id: None,
            remote_contact_id: None,
            card_type: value.card_type.into(),
            data: value.data,
            signature: value.signature,
            row_id: None,
            stash: None,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("contact_emails")]
pub struct ContactEmail {
    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    #[IdField(optional)]
    pub remote_id: Option<RemoteId>,

    /// TODO: Document this field.
    #[DbField]
    pub remote_contact_id: Option<RemoteId>,

    /// TODO: Document this field.
    #[DbField]
    pub canonical_email: String,

    /// TODO: Document this field.
    #[DbField]
    pub contact_type: ContactTypes,

    /// TODO: Document this field.
    #[DbField]
    pub defaults: ContactSendingPreferences,

    /// TODO: Document this field.
    #[DbField]
    pub display_order: u32,

    /// TODO: Document this field.
    #[DbField]
    pub email: String,

    /// TODO: Document this field.
    #[DbField]
    pub is_proton: bool,

    /// TODO: Document this field.
    #[DbField]
    pub label_ids: Labels,

    /// TODO: Document this field.
    #[DbField]
    pub last_used_time: u64,

    /// TODO: Document this field.
    #[DbField]
    pub name: String,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,

    /// The database instance that the record is associated with. This is
    /// present for convenience.
    #[StashField]
    pub stash: Option<Stash>,
}

impl From<ApiContactEmail> for ContactEmail {
    fn from(value: ApiContactEmail) -> Self {
        Self {
            remote_id: Some(value.id.into()),
            remote_contact_id: Some(value.contact_id.into()),
            canonical_email: value.canonical_email,
            contact_type: ContactTypes::new(value.contact_type),
            defaults: value.defaults.into(),
            display_order: value.order,
            email: value.email,
            is_proton: value.is_proton,
            label_ids: Labels::new(value.label_ids.into_iter().map(LabelId::from).collect()),
            last_used_time: value.last_used_time,
            name: value.name,
            row_id: None,
            stash: None,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("users")]
pub struct User {
    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    #[IdField(optional)]
    pub remote_id: Option<RemoteId>,

    /// TODO: Document this field.
    #[DbField]
    pub create_time: u64,

    /// TODO: Document this field.
    #[DbField]
    pub credit: i64,

    /// TODO: Document this field.
    #[DbField]
    pub currency: String,

    /// TODO: Document this field.
    #[DbField]
    pub delinquent: u32,

    /// TODO: Document this field.
    #[DbField]
    pub display_name: Option<String>,

    /// TODO: Document this field.
    #[DbField]
    pub email: String,

    /// TODO: Document this field.
    #[DbField]
    pub keys: UserKeys,

    /// TODO: Document this field.
    #[DbField]
    pub flags: Flags,

    /// TODO: Document this field.
    #[DbField]
    pub max_space: i64,

    /// TODO: Document this field.
    #[DbField]
    pub max_upload: i64,

    /// TODO: Document this field.
    #[DbField]
    pub mnemonic_status: UserMnemonicStatus,

    /// TODO: Document this field.
    #[DbField]
    pub private: u32,

    /// TODO: Document this field.
    #[DbField]
    pub name: Option<String>,

    /// TODO: Document this field.
    #[DbField]
    pub product_used_space: ProductUsedSpace,

    /// TODO: Document this field.
    #[DbField]
    pub role: u32,

    /// TODO: Document this field.
    #[DbField]
    pub services: u32,

    /// TODO: Document this field.
    #[DbField]
    pub subscribed: u32,

    /// TODO: Document this field.
    #[DbField]
    pub to_migrate: bool,

    /// TODO: Document this field.
    #[DbField]
    pub used_space: i64,

    /// TODO: Document this field.
    #[DbField]
    pub user_type: UserType,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,

    /// The database instance that the record is associated with. This is
    /// present for convenience.
    #[StashField]
    pub stash: Option<Stash>,
}

impl From<ApiUser> for User {
    fn from(value: ApiUser) -> Self {
        Self {
            remote_id: Some(value.id.into()),
            create_time: value.create_time,
            credit: value.credit,
            currency: value.currency,
            delinquent: value.delinquent,
            display_name: value.display_name,
            email: value.email,
            keys: value.keys.into(),
            flags: value.flags.into(),
            max_space: value.max_space,
            max_upload: value.max_upload,
            mnemonic_status: value.mnemonic_status.into(),
            private: value.private,
            name: value.name,
            product_used_space: value.product_used_space.into(),
            role: value.role,
            services: value.services,
            subscribed: value.subscribed,
            to_migrate: value.to_migrate,
            used_space: value.used_space,
            user_type: value.user_type.into(),
            row_id: None,
            stash: None,
        }
    }
}

impl User {
    // /// Get the user's display name.
    // #[must_use]
    // pub fn user_name(&self) -> &str {
    //     if let Some(display_name) = self.display_name.as_deref() {
    //         display_name
    //     } else if let Some(name) = self.name.as_deref() {
    //         name
    //     } else {
    //         &self.email
    //     }
    // }

    /// Download and store user info and settings into the database
    ///
    /// # Parameters
    ///
    /// * `stash` - The database instance to store the addresses.
    /// * `api`   - The API instance to use to download the addresses.
    ///
    /// # Errors
    ///
    /// TODO: Document the errors.
    ///
    pub async fn sync_user_and_settings(api: &Proton, stash: &Stash) -> CoreContextResult<()> {
        let mut user = User::from(api.get_users().await?.user);
        let mut settings = UserSettings::from(api.get_settings().await?.user_settings);
        settings.remote_id.clone_from(&user.remote_id);
        user.set_stash(stash);
        settings.set_stash(stash);
        user.save().await?;
        settings.save().await?;
        Ok(())
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("user_settings")]
#[allow(clippy::struct_excessive_bools)]
pub struct UserSettings {
    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    #[IdField(optional)]
    pub remote_id: Option<RemoteId>,

    /// TODO: Document this field.
    #[DbField]
    pub crash_reports: bool,

    /// TODO: Document this field.
    #[DbField]
    pub date_format: DateFormat,

    /// TODO: Document this field.
    #[DbField]
    pub density: Density,

    /// TODO: Document this field.
    #[DbField]
    pub device_recovery: bool,

    /// TODO: Document this field.
    #[DbField]
    pub early_access: bool,

    /// TODO: Document this field.
    #[DbField]
    pub email: Email,

    /// TODO: Document this field.
    #[DbField]
    pub flags: SettingsFlags,

    /// TODO: Document this field.
    #[DbField]
    pub hide_side_panel: bool,

    /// TODO: Document this field.
    #[DbField]
    pub high_security: HighSecurity,

    /// TODO: Document this field.
    #[DbField]
    pub invoice_text: String,

    /// TODO: Document this field.
    #[DbField]
    pub locale: String,

    /// TODO: Document this field.
    #[DbField]
    pub log_auth: LogAuth,

    /// TODO: Document this field.
    #[DbField]
    pub news: u32,

    /// TODO: Document this field.
    #[DbField]
    pub password: Password,

    /// TODO: Document this field.
    #[DbField]
    pub phone: Phone,

    /// TODO: Document this field.
    #[DbField]
    pub referral: Option<Referral>,

    /// TODO: Document this field.
    #[DbField]
    pub session_account_recovery: bool,

    /// TODO: Document this field.
    #[DbField]
    pub telemetry: bool,

    /// TODO: Document this field.
    #[DbField]
    pub time_format: TimeFormat,

    /// TODO: Document this field.
    #[DbField]
    pub two_factor_auth: TwoFa,

    /// TODO: Document this field.
    #[DbField]
    pub week_start: WeekStart,

    /// TODO: Document this field.
    #[DbField]
    pub welcome: bool,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,

    /// The database instance that the record is associated with. This is
    /// present for convenience.
    #[StashField]
    pub stash: Option<Stash>,
}

impl From<ApiUserSettings> for UserSettings {
    fn from(value: ApiUserSettings) -> Self {
        Self {
            remote_id: None,
            crash_reports: value.crash_reports,
            date_format: value.date_format.into(),
            density: value.density.into(),
            device_recovery: value.device_recovery,
            early_access: value.early_access,
            email: value.email.into(),
            flags: value.flags.into(),
            hide_side_panel: value.hide_side_panel,
            high_security: value.high_security.into(),
            invoice_text: value.invoice_text,
            locale: value.locale,
            log_auth: value.log_auth.into(),
            news: value.news,
            password: value.password.into(),
            phone: value.phone.into(),
            referral: value.referral.map(Into::into),
            session_account_recovery: value.session_account_recovery,
            telemetry: value.telemetry,
            time_format: value.time_format.into(),
            two_factor_auth: value.two_factor_auth.into(),
            week_start: value.week_start.into(),
            welcome: value.welcome,
            row_id: None,
            stash: None,
        }
    }
}
