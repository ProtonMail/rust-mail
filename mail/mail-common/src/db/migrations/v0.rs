//! V0 Initial db creation
mod attachments;
mod conversations;
mod events;
mod labels;
mod messages;
mod rollback_actions;
mod settings;

use stash::stash::{Bond, StashError};
use tracing::debug_span;

pub struct MigrationV0 {}

impl proton_sqlite3::Migration for MigrationV0 {
    fn name(&self) -> &str {
        "proton_mail_db_v0"
    }

    async fn migrate(&self, tx: &Bond<'_>) -> Result<(), StashError> {
        let span = debug_span!("labels");
        let entered = span.enter();
        labels::create_labels_tables(tx).await?;
        drop(entered);
        let span = debug_span!("attachments");
        let entered = span.enter();
        attachments::create_attachment_tables(tx).await?;
        drop(entered);
        let span = debug_span!("conversations");
        let entered = span.enter();
        conversations::create_conversation_tables(tx).await?;
        drop(entered);
        let span = debug_span!("messages");
        let entered = span.enter();
        messages::create_message_tables(tx).await?;
        drop(entered);
        let span = debug_span!("events");
        let entered = span.enter();
        events::create_event_tables(tx).await?;
        drop(entered);
        let span = debug_span!("settings");
        let entered = span.enter();
        settings::create_settings_table(tx).await?;
        drop(entered);
        let span = debug_span!("rollback_actions");
        let entered = span.enter();
        rollback_actions::create_rollback_action_tables(tx).await?;
        drop(entered);
        Ok(())
    }
}
