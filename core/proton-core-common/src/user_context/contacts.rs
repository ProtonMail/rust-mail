use proton_api_core::domain::{ContactFilter, ContactId};
use proton_event_loop::proton_async::futures::TryFutureExt;

use crate::db::CoreSqliteConnection;
use crate::{CoreContextResult, UserContext};

const SYNC_CONTACT_PAGE_SIZE: usize = 1000;

impl UserContext {
    /// Updates all user contacts including their emails without their cards.
    ///
    /// # Errors
    /// Returns error if the operation failed.
    pub async fn sync_contacts(&self) -> CoreContextResult<()> {
        let mut page_index = 0;
        let mut connection = self.new_db_connection_as::<CoreSqliteConnection>()?;
        // First update the partial contacts since email contacts reference them.
        loop {
            let filter: ContactFilter =
                ContactFilter::new_builder(page_index, SYNC_CONTACT_PAGE_SIZE).build();
            let contacts = self.session.contacts(filter).await?;
            connection.tx(|tx| tx.create_or_update_partial_contacts(contacts.iter()))?;
            if contacts.len() < SYNC_CONTACT_PAGE_SIZE {
                break;
            }
            page_index += 1;
        }

        // Then, update the email contacts.
        page_index = 0;
        loop {
            let filter: ContactFilter =
                ContactFilter::new_builder(page_index, SYNC_CONTACT_PAGE_SIZE).build();
            let contact_emails = self.session.contact_emails(filter).await?;
            connection.tx(|tx| tx.create_or_update_contact_emails(contact_emails.iter()))?;
            if contact_emails.len() < SYNC_CONTACT_PAGE_SIZE {
                break;
            }
            page_index += 1;
        }
        Ok(())
    }

    /// Updates the full contact with the given id including its emails and cards.
    ///
    /// # Errors
    /// Returns error if the operation failed.
    pub async fn sync_contact_with_card(&self, id: ContactId) -> CoreContextResult<()> {
        let mut connection = self.new_db_connection_as::<CoreSqliteConnection>()?;
        self.session
            .contact_with_cards(id)
            .map_ok(|contact_with_card| {
                connection.tx(|tx| tx.create_or_update_contact(&contact_with_card))
            })
            .await??;
        Ok(())
    }
}
