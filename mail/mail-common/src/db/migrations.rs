mod v001_proton_mail_default_labels;
mod v005_proton_mail_conversation_counters;
mod v007_proton_mail_message_counters;
mod v016_proton_mail_new_system_labels;
mod v019_proton_mail_draft_send_result_refactor;
mod v045_proton_mail_draft_send_result;
mod v046_proton_mail_android_signatures;

use include_dir::{Dir, include_dir};
use proton_sqlite3::{Migrator, MigratorError, file::embedded_migrations};
use stash::stash::Stash;

pub async fn migrate_db(stash: &Stash) -> Result<usize, MigratorError> {
    const TABLE: &str = "proton_mail_db_version";
    const MIGRATIONS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/src/db/migrations");

    let mut migrations = embedded_migrations(&MIGRATIONS);

    migrations.push(Box::new(
        v001_proton_mail_default_labels::DefaultLabelsMigration,
    ));
    migrations.push(Box::new(
        v005_proton_mail_conversation_counters::ConversationCountersMigration,
    ));
    migrations.push(Box::new(
        v007_proton_mail_message_counters::MessageCountersMigration,
    ));
    migrations.push(Box::new(
        v016_proton_mail_new_system_labels::DefaultLabelsMigration,
    ));
    migrations.push(Box::new(
        v019_proton_mail_draft_send_result_refactor::DraftSendResultMigration,
    ));
    migrations.push(Box::new(
        v045_proton_mail_draft_send_result::DraftSendResultAttachmentErrorsMigration,
    ));
    migrations.push(Box::new(
        v046_proton_mail_android_signatures::AndroidSignaturesMigration,
    ));

    let mut tether = stash.connection().await?;

    Migrator::new(TABLE, migrations).migrate(&mut tether).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use proton_core_common::db::migrations::migrate_core_db;

    #[tokio::test]
    async fn test_migration_on_empty_data_set() {
        let stash = Stash::new(None).expect("Failed to create Stash");

        migrate_core_db(&stash).await.expect("failed to migrate");
        migrate_db(&stash).await.expect("failed to migrate");
    }
}
