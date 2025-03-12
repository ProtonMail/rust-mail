//! V0 Initial db creation
mod attachments;
mod conversations;
mod default_labels;
mod events;
mod mailbox_labels;
mod messages;
mod rollback_actions;
mod scroller;
mod settings;

use stash::stash::{Bond, StashError};
use tracing::{Instrument, debug_span};

pub struct MigrationV0 {}

impl proton_sqlite3::Migration for MigrationV0 {
    fn name(&self) -> &str {
        "proton_mail_db_v0"
    }

    async fn migrate(&self, tx: &Bond<'_>) -> Result<(), StashError> {
        default_labels::create_default_labels(tx)
            .instrument(debug_span!("default_labels"))
            .await?;

        mailbox_labels::create_mailbox_labels(tx)
            .instrument(debug_span!("mailbox_labels"))
            .await?;

        attachments::create_attachment_tables(tx)
            .instrument(debug_span!("attachments"))
            .await?;

        conversations::create_conversation_tables(tx)
            .instrument(debug_span!("conversations"))
            .await?;

        messages::create_message_tables(tx)
            .instrument(debug_span!("messages"))
            .await?;

        events::create_event_tables(tx)
            .instrument(debug_span!("events"))
            .await?;

        settings::create_settings_table(tx)
            .instrument(debug_span!("settings"))
            .await?;

        rollback_actions::create_rollback_action_tables(tx)
            .instrument(debug_span!("rollback_actions"))
            .await?;

        scroller::create_paginator_tables(tx)
            .instrument(debug_span!("paginator"))
            .await?;

        Ok(())
    }
}
