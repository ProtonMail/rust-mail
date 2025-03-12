use proton_sqlite3::Migration;
use stash::stash::{Bond, StashError};
use tracing::{Instrument, debug_span};

mod addresses;
mod contacts;
mod labels;
mod sender_image_cache;
mod user;
mod user_settings;

pub struct V0 {}

impl Migration for V0 {
    fn name(&self) -> &'static str {
        "proton_core_v0"
    }

    async fn migrate(&self, tx: &Bond<'_>) -> Result<(), StashError> {
        addresses::create_tables(tx)
            .instrument(debug_span!("addresses"))
            .await?;

        user_settings::create_tables(tx)
            .instrument(debug_span!("user_settings"))
            .await?;

        user::create_tables(tx)
            .instrument(debug_span!("user"))
            .await?;

        contacts::create_tables(tx)
            .instrument(debug_span!("contacts"))
            .await?;

        sender_image_cache::create_tables(tx)
            .instrument(debug_span!("sender_image_cache"))
            .await?;

        labels::create_labels_tables(tx)
            .instrument(debug_span!("labels"))
            .await?;

        Ok(())
    }
}
