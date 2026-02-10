use std::collections::HashSet;

use crate::{
    CoreContextError,
    models::{Contact, ContactEmail},
};
use proton_core_api::{
    services::proton::{ContactEmail as ApiContactEmail, ContactId},
    session::Session,
};
use stash::{
    UserDb,
    stash::{RunTransaction, Tether},
};
use stash::{orm::Model, params};

#[derive(Default)]
pub struct ContactsDependencyFetcher {
    contact_ids: HashSet<ContactId>,
}

impl ContactsDependencyFetcher {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl ContactsDependencyFetcher {
    pub async fn fetch_and_store(
        &self,
        api: &Session,
        tx: &mut (impl RunTransaction<UserDb> + Send),
    ) -> Result<(), CoreContextError> {
        if !self.contact_ids.is_empty() {
            Contact::sync_contacts_by_ids(api, self.contact_ids.iter().cloned().collect(), tx)
                .await?;
        }
        Ok(())
    }

    pub async fn check_contact_email(
        &mut self,
        contact_email: &ContactEmail,
        tether: &Tether,
    ) -> Result<(), CoreContextError> {
        if let Some(contact_id) = contact_email.remote_contact_id.as_ref() {
            self.check_contact_id(contact_id, tether).await?;
        }
        Ok(())
    }

    pub async fn check_api_contact_email(
        &mut self,
        contact_email: &ApiContactEmail,
        tether: &Tether,
    ) -> Result<(), CoreContextError> {
        self.check_contact_id(&contact_email.contact_id, tether)
            .await
    }

    async fn check_contact_id(
        &mut self,
        contact_id: &ContactId,
        tether: &Tether,
    ) -> Result<(), CoreContextError> {
        let contact =
            Contact::find_first("WHERE remote_id = ?", params![contact_id.clone()], tether).await?;

        if contact.is_none() {
            self.contact_ids.insert(contact_id.clone());
        }

        Ok(())
    }
}
