use std::sync::Arc;

use clap::Parser;
use proton_action_queue::observers::ActionAwaiter;
use proton_action_queue::queue::BroadcastMessage;
use proton_core_common::Origin;
use proton_core_common::datatypes::{ApiConfig, AppDetails};
use proton_core_common::db::account::SessionEncryptionKey;
use proton_core_common::event_loop::EventPollMode;
use proton_core_common::os::{InMemoryKeyChain, KeyChainExt};
use proton_issue_reporter_service::NoopIssueReporter;
use proton_log_service::LogService;
use proton_mail_common::MailContext;
use proton_mail_common::datatypes::Disposition;
use proton_mail_common::draft::recipients::RecipientEntry;
use proton_mail_common::draft::{Draft, RecipientGroupId};
use proton_mail_common::models::Attachment;
use tempfile::TempDir;
use tokio::runtime;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    username: String,
    #[arg(short, long)]
    password: String,
    #[clap(short, long)]
    subject: String,
    #[clap(short, long)]
    recipient: String,
    #[clap(short, long)]
    body: String,
    #[clap(long)]
    email_password: Option<String>,
}
#[tokio::main]
async fn main() {
    let env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::DEBUG.into())
        .parse_lossy(
            "info,proton_sqlite3=trace,\
                proton_core_common=trace,proton_mail_common=trace,\
                proton_event_loop=trace,proton_core_api=trace,\
                proton_action_queue=trace,proton_mail_api=trace,\
                stash=error",
        );
    tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter(env_filter)
        .init();

    let Args {
        username,
        password,
        subject,
        recipient,
        mut body,
        email_password,
    } = Args::parse();
    let tmp_dir = TempDir::new().unwrap();
    let tmp_file = tmp_dir.path().join("hello_world.txt");

    let keychain = InMemoryKeyChain::default();
    let key = SessionEncryptionKey::random();
    keychain.store(key).unwrap();

    info!("TMP DIR: {:?}", tmp_dir.path());

    let config = proton_log_service::Config::builder()
        .name("log".into())
        .directory(tmp_dir.path().into())
        .build();
    let api_config = ApiConfig {
        app_details: AppDetails {
            platform: "ios".into(),
            product: "mail".into(),
            version: "7.1.0".into(),
        },
        ..Default::default()
    };

    let ctx = MailContext::new(
        Origin::App,
        runtime::Handle::current(),
        tmp_dir.path().join("session"),
        tmp_dir.path().join("user"),
        tmp_dir.path().join("core_cache"),
        tmp_dir.path().join("mail_cache"),
        50 * 1204 * 1024,
        Arc::new(keychain),
        api_config,
        None,
        None,
        LogService::new(config),
        EventPollMode::Manual,
        Default::default(),
        Arc::new(NoopIssueReporter),
    )
    .await
    .unwrap();

    let mut flow = ctx.new_login_flow().await.unwrap();

    flow.login_with_credentials(username, password, None)
        .await
        .unwrap();

    let user_ctx = ctx.user_context_from_login_flow(&mut flow).await.unwrap();

    let draft = Draft::empty(&user_ctx).await.unwrap();
    if let Some(email_password) = email_password {
        draft.set_password(&email_password, None).await.unwrap();
    }

    draft.set_subject(subject).await.unwrap();
    body.push_str(draft.body().await.unwrap().as_ref());
    draft.set_body(body).await.unwrap();
    draft
        .add_single_recipient(
            RecipientGroupId::To,
            RecipientEntry {
                display_name: None,
                email: recipient.into(),
            },
        )
        .await
        .unwrap();

    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    tokio::fs::write(&tmp_file, b"Hello world attachment")
        .await
        .unwrap();

    let id = draft.save().await.unwrap().id;

    ActionAwaiter::new(user_ctx.action_queue(), id)
        .wait()
        .await
        .unwrap();

    // Add attachment after save.
    let attachment = Attachment::create_local(
        &user_ctx,
        draft.address_id().await.unwrap(),
        Disposition::Attachment,
        &tmp_file,
        None,
        &mut tether,
    )
    .await
    .unwrap();

    draft.add_attachment(&attachment).await.unwrap();

    let id = draft.send().await.unwrap().id;

    let send_awaiter = ActionAwaiter::new(user_ctx.action_queue(), id);

    match send_awaiter.wait().await.unwrap() {
        BroadcastMessage::Queued(_, _) => {}
        BroadcastMessage::Success(_) => {
            info!("Message successfully sent.");
        }
        BroadcastMessage::Error(err, _) => {
            error!("Error sending message: {:?}", err);
        }
        BroadcastMessage::Cancelled(_) => {
            info!("Sending Cancelled.");
        }
        BroadcastMessage::Deleted(_, _) => {
            info!("Sending Action Deleted.");
        }
    }
}
