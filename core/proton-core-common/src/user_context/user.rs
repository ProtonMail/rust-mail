use crate::db::CoreSqliteConnection;
use crate::{CoreContextResult, UserContext};

impl UserContext {
    /// Download and store user info and settings into the database
    ///
    /// # Errors
    /// Returns error if the operation failed.
    pub async fn sync_user_and_settings(&self) -> CoreContextResult<()> {
        let user = self.session.get_user().await?;
        let settings = self.session.get_user_settings().await?;
        let mut conn = self.new_db_connection_as::<CoreSqliteConnection>()?;

        conn.tx(|tx| {
            tx.create_or_update_user(&user)?;
            tx.create_or_update_user_settings(&self.user_id, &settings)
        })?;
        Ok(())
    }
}
