use std::sync::Arc;

use clap::Parser;
use proton_action_queue::observers::ActionAwaiter;
use proton_action_queue::queue::BroadcastMessage;
use proton_core_api::services::proton::muon::client::flow::LoginExtraInfo;
use proton_core_api::session::Config;
use proton_core_common::db::account::SessionEncryptionKey;
use proton_core_common::event_loop::EventPollMode;
use proton_core_common::os::{InMemoryKeyChain, KeyChainExt};
use proton_mail_common::MailContext;
use proton_mail_common::datatypes::Disposition;
use proton_mail_common::draft::Draft;
use proton_mail_common::draft::recipients::{MaybeEmptyString, RecipientEntry};
use proton_mail_common::models::Attachment;
use tempdir::TempDir;
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
    } = Args::parse();
    let tmp_dir = TempDir::new("cli").unwrap();
    let tmp_file = tmp_dir.path().join("hello_world.txt");

    let keychain = InMemoryKeyChain::default();
    let key = SessionEncryptionKey::random();
    keychain.store(key).unwrap();

    info!("TMP DIR: {:?}", tmp_dir.path());

    let ctx = MailContext::new(
        tmp_dir.path().join("session"),
        tmp_dir.path().join("user"),
        tmp_dir.path().join("core_cache"),
        tmp_dir.path().join("mail_cache"),
        50 * 1204 * 1024,
        None,
        Arc::new(keychain),
        Config::default(),
        None,
        None,
        None,
        EventPollMode::Manual,
    )
    .await
    .unwrap();

    let mut flow = ctx.new_login_flow().await.unwrap();

    flow.login(username, password, LoginExtraInfo::default())
        .await
        .unwrap();

    let user_ctx = ctx.user_context_from_login_flow(&mut flow).await.unwrap();

    let mut draft = Draft::empty(&user_ctx).await.unwrap();

    draft.subject = subject;
    body.push_str(draft.body());
    draft.set_body(body);
    draft
        .to_list
        .add_single(RecipientEntry {
            display_name: MaybeEmptyString(None),
            email: recipient,
        })
        .unwrap();

    let mut tether = user_ctx.user_stash().connection();

    tokio::fs::write(&tmp_file, b"Hello world attachment")
        .await
        .unwrap();

    let id = draft
        .save(user_ctx.action_queue(), &tether)
        .await
        .unwrap()
        .id;
    ActionAwaiter::new(user_ctx.action_queue(), id)
        .wait()
        .await
        .unwrap();

    // Add attachment after save.
    let attachment = Attachment::create_local(
        &user_ctx,
        draft.address_id.clone(),
        Disposition::Attachment,
        &tmp_file,
        None,
        &mut tether,
    )
    .await
    .unwrap();

    draft.add_attachment(&user_ctx, attachment).await.unwrap();

    let id = draft
        .send(user_ctx.action_queue(), &user_ctx.user_stash().connection())
        .await
        .unwrap()
        .id;
    let send_awaiter = ActionAwaiter::new(user_ctx.action_queue(), id);
    match send_awaiter.wait().await.unwrap() {
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
