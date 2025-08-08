use crate::models::Message;
use proton_core_common::datatypes::SystemLabel;
use proton_sqlite3::Migration;
use stash::{
    orm::Model,
    stash::{Bond, StashError},
};

pub struct FixSnoozeTimeMessagesMigration;

#[async_trait::async_trait]
impl Migration for FixSnoozeTimeMessagesMigration {
    fn name(&self) -> &str {
        "v035_proton_mail_fix_snooze_time_messages"
    }

    async fn migrate(&self, tx: &Bond<'_>) -> Result<(), StashError> {
        let inbox_label_id = SystemLabel::Inbox
            .local_id(tx)
            .await?
            .expect("Inbox should be set");

        let messages = Message::in_label(inbox_label_id, tx).await?;

        for mut message in messages {
            message.save(tx).await?;
        }

        Ok(())
    }
}
