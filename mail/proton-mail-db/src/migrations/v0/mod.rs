//! V0 Initial db creation
mod addresses;
mod attachments;
mod conversations;
mod events;
mod labels;
mod messages;
mod settings;

use proton_api_mail::proton_api_core::exports::tracing;
use proton_sqlite3::rusqlite::Transaction;

pub struct MigrationV0 {}

impl proton_sqlite3::Migration for MigrationV0 {
    fn name(&self) -> &str {
        "proton_mail_db_v0"
    }

    fn migrate(&self, tx: &mut Transaction) -> proton_sqlite3::rusqlite::Result<()> {
        tracing::debug_span!("labels").in_scope(|| labels::create_labels_tables(tx))?;
        tracing::debug_span!("labels").in_scope(|| addresses::create_addresses_tables(tx))?;
        tracing::debug_span!("attachments")
            .in_scope(|| attachments::create_attachment_tables(tx))?;
        tracing::debug_span!("conversations")
            .in_scope(|| conversations::create_conversation_tables(tx))?;
        tracing::debug_span!("messages").in_scope(|| messages::create_message_tables(tx))?;
        tracing::debug_span!("events").in_scope(|| events::create_event_tables(tx))?;
        tracing::debug_span!("settings").in_scope(|| settings::create_settings_table(tx))?;
        Ok(())
    }
}
