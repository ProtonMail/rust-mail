mod fetch_address_flags;
mod fetch_contact_emails;

use crate::{MailContextError, MailContextResult, MailUserContext};
use anyhow::anyhow;
use itertools::Itertools;
use proton_core_common::models::ModelExtension;
use proton_issue_reporter_service::{IssueLevel, IssueReportKeys};
use stash::{UserDb, macros::Model};
use std::sync::{Arc, Weak};
use tracing::{Instrument, debug, error, info, instrument};

#[derive(Clone, Debug, Eq, PartialEq, Model)]
#[TableName("pending_online_migrations")]
#[Database(UserDb)]
struct PendingOnlineMigration {
    #[IdField]
    name: String,
}

#[instrument(skip_all)]
pub async fn run(ctx: &Arc<MailUserContext>) -> MailContextResult<()> {
    let migrations = {
        let tether = ctx.user_stash().connection().await?;

        PendingOnlineMigration::all(&tether).await?
    };

    if migrations.is_empty() {
        debug!("Got no pending online migrations");

        return Ok(());
    }

    info!(
        "Got {} pending online migration(s): {}",
        migrations.len(),
        migrations.iter().map(|m| &m.name).join(", ")
    );

    ctx.spawn(
        {
            let ctx = ctx.as_weak();

            async move {
                for migration in migrations {
                    try_migrate(&ctx, migration).await;
                }

                info!("Migrations completed");

                #[cfg(test)]
                tests::SIGNAL.with(|tx| {
                    _ = tx.lock().take().unwrap().send(());
                });
            }
        }
        .in_current_span(),
    );

    Ok(())
}

#[instrument(name = "migrate", skip_all, fields(name = ?migration.name))]
async fn try_migrate(ctx: &Weak<MailUserContext>, migration: PendingOnlineMigration) {
    info!("Starting migration");

    let name = migration.name.clone();

    match migrate(ctx, migration).await {
        Ok(()) => {
            info!("Migration completed");
        }

        Err(err) => {
            error!(?err, "Migration failed");

            if let Some(ctx) = ctx.upgrade() {
                ctx.user_context().issue_reporter_service().report(
                    IssueLevel::Critical,
                    "Couldn't execute an online-migration".into(),
                    IssueReportKeys::from([
                        ("name".into(), name),
                        ("error".into(), format!("{err:?}")),
                    ]),
                );
            }
        }
    }
}

async fn migrate(
    ctx: &Weak<MailUserContext>,
    migration: PendingOnlineMigration,
) -> MailContextResult<()> {
    match migration.name.as_str() {
        "fetch-address-flags" => fetch_address_flags::run(ctx).await?,
        "fetch-contact-emails" => fetch_contact_emails::run(ctx).await?,

        #[cfg(test)]
        "test-ok" => {}

        #[cfg(test)]
        "test-err" => {
            return Err(MailContextError::Other(anyhow!("Migration failed")));
        }

        name => {
            return Err(MailContextError::Other(anyhow!(
                "Unknown online migration `{name}`"
            )));
        }
    };

    let mut tether = ctx
        .upgrade()
        .ok_or(MailContextError::LostContext)?
        .user_stash()
        .connection()
        .await?;

    tether.tx(async |bond| migration.delete(bond).await).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::test_utils::test_context::MailTestContext;
    use parking_lot::Mutex;
    use proton_core_api::services::proton::{
        ContactEmail as ApiContactEmail, ContactEmailId, ContactFull, ContactId,
        ContactSendingPreferences as ApiContactSendingPreferences, ContactUID, PrivateEmail,
    };
    use proton_core_common::{
        datatypes::{ContactSendingPreferences, ContactTypes, Labels, UnixTimestamp},
        models::{Contact, ContactEmail},
    };
    use stash::orm::Model;
    use std::{time::Duration, vec};
    use tokio::{sync::oneshot, time};

    // Exploits the fact that `#[tokio::test]` creates a current-thread runtime
    thread_local! {
        pub static SIGNAL: Mutex<Option<oneshot::Sender<()>>> = const { Mutex::new(None) };
    }

    #[tokio::test]
    async fn smoke() {
        let ctx = MailTestContext::new().await;
        let tether = ctx.user_context().await.stash().connection().await.unwrap();

        tether
            .execute(
                "INSERT INTO pending_online_migrations (name) VALUES ('test-ok'), ('test-err')",
                Vec::new(),
            )
            .await
            .unwrap();

        drop(tether);

        // ---

        let (tx, rx) = oneshot::channel();

        SIGNAL.with(|signal| {
            *signal.lock() = Some(tx);
        });

        // ---

        let _muctx = ctx.uninitialized_mail_user_context().await;

        time::timeout(Duration::from_secs(10), rx)
            .await
            .unwrap()
            .unwrap();

        let actual = ctx
            .user_context()
            .await
            .stash()
            .connection()
            .await
            .unwrap()
            .query_values::<_, String>(
                "SELECT name AS value FROM pending_online_migrations",
                Vec::new(),
            )
            .await
            .unwrap();

        assert_eq!(vec!["test-err".to_string()], actual);
    }

    #[tokio::test]
    async fn missing_contacts() {
        let ctx = MailTestContext::new().await;

        let user_ctx = ctx.uninitialized_mail_user_context().await;
        let (tx, rx) = oneshot::channel();

        SIGNAL.with(|signal| {
            *signal.lock() = Some(tx);
        });
        let mut tether = user_ctx.user_stash().connection().await.unwrap();

        let remote_id1 = ContactId::from("Contact1");
        let remote_id2 = ContactId::from("Contact2");

        let mut contact1 = Contact {
            local_id: None,
            remote_id: Some(remote_id1.clone()),
            cards: vec![],
            contact_emails: vec![],
            create_time: 1024,
            label_ids: Labels::new(vec![]),
            modify_time: 1024,
            name: "Contact1".to_owned(),
            size: 100,
            uid: ContactUID::from("UID1"),
            deleted: false,
        };
        let mut contact2 = Contact {
            local_id: None,
            remote_id: Some(remote_id2.clone()),
            cards: vec![],
            contact_emails: vec![ContactEmail {
                local_id: None,
                remote_id: Some(ContactEmailId::from("email")),
                remote_contact_id: Some(remote_id2.clone()),
                local_contact_id: None,
                canonical_email: PrivateEmail::new("foo@bar.com"),
                contact_type: ContactTypes::new(vec![]),
                defaults: ContactSendingPreferences::Default,
                display_order: 0,
                email: PrivateEmail::new("foo@bar.com"),
                is_proton: false,
                label_ids: Labels::new(vec![]),
                last_used_time: UnixTimestamp::now(),
                name: "Hello".to_owned(),
            }],
            create_time: 1024,
            label_ids: Labels::new(vec![]),
            modify_time: 1024,
            name: "Contact1".to_owned(),
            size: 100,
            uid: ContactUID::from("UID1"),
            deleted: false,
        };

        let contact_ids_wihtout_email = tether
            .tx(async |tx| {
                contact1.save(tx).await?;
                contact2.save(tx).await?;

                Contact::without_emails(tx).await
            })
            .await
            .unwrap();

        assert_eq!(contact_ids_wihtout_email, vec![contact1.id()]);

        time::timeout(Duration::from_secs(10), rx)
            .await
            .unwrap()
            .unwrap();

        tether
            .execute(
                "INSERT OR IGNORE INTO pending_online_migrations (name) VALUES ('fetch-contact-emails')",
                Vec::new(),
            )
            .await
            .unwrap();

        drop(tether);
        drop(user_ctx);

        // ---

        ctx.core_test_context
            .mock_get_full_contact(ContactFull {
                id: remote_id1.clone(),
                cards: vec![],
                contact_emails: vec![ApiContactEmail {
                    id: ContactEmailId::from("conact1_email"),
                    contact_id: remote_id1.clone(),
                    canonical_email: PrivateEmail::new("bar@bar.com"),
                    contact_type: vec![],
                    defaults: ApiContactSendingPreferences::Default,
                    email: PrivateEmail::new("bar@bar.com"),
                    is_proton: false,
                    label_ids: vec![],
                    last_used_time: 0,
                    name: "Bar".to_owned(),
                    order: 1,
                }],
                create_time: 1024,
                label_ids: vec![],
                modify_time: 1024,
                name: "Contact1".to_owned(),
                size: 100,
                uid: contact1.uid.clone(),
            })
            .await;

        let (tx, rx) = oneshot::channel();

        SIGNAL.with(|signal| {
            *signal.lock() = Some(tx);
        });

        // ---

        let _muctx = ctx.uninitialized_mail_user_context().await;

        time::timeout(Duration::from_secs(10), rx)
            .await
            .unwrap()
            .unwrap();
    }
}
