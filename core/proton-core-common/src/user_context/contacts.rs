use crate::{CoreContextResult, UserContext};
use proton_api_core::domain::{ContactFilter, ContactId};
use proton_api_core::exports::tracing::{self, debug, error, Level};
use stash::orm::Model;
use stash::stash::StashError;

const SYNC_CONTACT_PAGE_SIZE: usize = 1000;

impl UserContext {
    /// Updates all user contacts including their emails without their cards.
    ///
    /// The update includes a reset of the database.
    ///
    /// # Errors
    /// Returns error if the operation failed.
    #[tracing::instrument(level = Level::DEBUG, skip(self))]
    pub async fn sync_contacts(&self) -> CoreContextResult<()> {
        // TODO: There should be one transaction for the whole sync.
        let mut page_index = 0;
        // Reset the database state by deleting all contacts.
        async {
            let tx = self.stash.transaction().await?;
            tx.execute("DELETE FROM contacts", vec![]).await?;
            tx.execute("DELETE FROM contact_emails", vec![]).await?;
            tx.execute("DELETE FROM contact_cards", vec![]).await?;
            tx.execute("DELETE FROM contact_email_labels", vec![])
                .await?;
            tx.commit().await?;
            Ok(())
        }
        .await
        .map_err(|err: StashError| {
            error!("Failed to reset contact tables: {err}");
            err
        })?;
        // First update the partial contacts since email contacts reference them.
        debug!("Syncing partial contacts");
        loop {
            let filter: ContactFilter =
                ContactFilter::new_builder(page_index, SYNC_CONTACT_PAGE_SIZE).build();
            let mut contacts = self.session.contacts(filter).await.map_err(|err| {
                error!("Failed to fetch contacts for page {page_index}: {err}");
                err
            })?;
            if !contacts.is_empty() {
                async {
                    let tx = self.stash.transaction().await?;
                    for contact in &mut contacts {
                        contact.save_using(&tx).await?;
                    }
                    tx.commit().await?;
                    Ok(())
                }
                .await
                .map_err(|err: StashError| {
                    error!("Failed to sync contacts for page {page_index} to db: {err}");
                    err
                })?;
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
            let filter: ContactFilter =
                ContactFilter::new_builder(page_index, SYNC_CONTACT_PAGE_SIZE).build();
            let mut contact_emails = self.session.contact_emails(filter).await.map_err(|err| {
                error!("Failed to sync contact emails for page {page_index}: {err}");
                err
            })?;
            if !contact_emails.is_empty() {
                async {
                    let tx = self.stash.transaction().await?;
                    for contact_email in &mut contact_emails {
                        contact_email.save_using(&tx).await?;
                    }
                    tx.commit().await?;
                    Ok(())
                }
                .await
                .map_err(|err: StashError| {
                    error!("Failed to sync contact emails for page {page_index} to db: {err}");
                    err
                })?;
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

    /// Updates the full contact with the given id including its emails and cards.
    ///
    /// # Errors
    /// Returns error if the operation failed.
    #[tracing::instrument(level = Level::DEBUG, skip(self))]
    pub async fn sync_contact_with_card(&self, id: ContactId) -> CoreContextResult<()> {
        debug!("Syncing full contact for contact id {id}");
        let mut contact_with_card = self.session.contact_with_cards(id).await.map_err(|err| {
            error!("Failed to fetch full contact with: {err}");
            err
        })?;
        contact_with_card.set_stash(&self.stash);
        contact_with_card.save().await.map_err(|err| {
            error!("Failed to sync full contact to db: {err}");
            err
        })?;
        Ok(())
    }
}
