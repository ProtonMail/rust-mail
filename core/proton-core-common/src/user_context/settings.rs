use crate::{CoreContextResult, UserContext};
use stash::orm::Model;

impl UserContext {
    /// Download and store user info and settings into the database
    ///
    /// # Errors
    /// Returns error if the operation failed.
    pub async fn sync_user_and_settings(&self) -> CoreContextResult<()> {
        let mut user = self.session.get_user().await?;
        let mut settings = self.session.get_user_settings().await?;
        let tx = self.stash.transaction().await?;
        user.save_using(&tx).await?;
        settings.save_using(&tx).await?;
        tx.commit().await?;
        Ok(())
    }
}
