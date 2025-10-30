use std::collections::HashSet;

use crate::{
    CoreContextError,
    models::{Contact, ContactEmail},
};
use proton_core_api::{services::proton::ContactId, session::Session};
use stash::stash::{RunTransaction, Tether};
use stash::{orm::Model, params};

pub struct ContactsDependencyFetcher {
    contact_ids: HashSet<ContactId>,
}

impl ContactsDependencyFetcher {
    pub fn new() -> Self {
        Self {
            contact_ids: HashSet::new(),
        }
    }
}

impl ContactsDependencyFetcher {
    pub async fn fetch_and_store(
        &self,
        api: &Session,
        tx: &mut (impl RunTransaction + Send),
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
