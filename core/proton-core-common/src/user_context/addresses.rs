use crate::{CoreContextResult, UserContext};
use stash::orm::Model;

impl UserContext {
    /// Download and store user addresses into the database
    ///
    /// # Errors
    /// Returns error if the operation failed.
    pub async fn sync_addresses(&self) -> CoreContextResult<()> {
        for mut address in self.session.addresses().await? {
            address.save().await?;
        }

        Ok(())
    }
}
