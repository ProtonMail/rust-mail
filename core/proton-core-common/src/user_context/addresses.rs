use crate::db::CoreSqliteConnection;
use crate::{CoreContextResult, UserContext};

impl UserContext {
    /// Download and store user addresses into the database
    ///
    /// # Errors
    /// Returns error if the operation failed.
    pub async fn sync_addresses(&self) -> CoreContextResult<()> {
        let addresses = self.session.addresses().await?;

        let mut connection = self.new_db_connection_as::<CoreSqliteConnection>()?;
        connection.tx(|tx| tx.create_or_update_addresses(addresses.iter()))?;

        Ok(())
    }
}
