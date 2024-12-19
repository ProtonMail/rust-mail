use clap::Parser;
use proton_api_core::session::Config;
use proton_core_common::db::account::SessionEncryptionKey;
use proton_core_common::os::{InMemoryKeyChain, KeyChain};
use proton_mail_common::draft::recipients::{MaybeEmptyString, RecipientEntry};
use proton_mail_common::draft::Draft;
use proton_mail_common::{
    MailContext, MailContextError, MailUserContext, MailUserContextInitializationCallback,
    MailUserContextLoadingStage,
};
use std::sync::Arc;
use tempdir::TempDir;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::EnvFilter;

struct InitCallback;

impl MailUserContextInitializationCallback for InitCallback {
    fn on_stage(&self, stage: MailUserContextLoadingStage) {
        tracing::info!("Init: {stage:?}");
    }

    fn on_stage_err(&self, _: MailUserContextLoadingStage, _: MailContextError) {}
}

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
        .with_default_directive(LevelFilter::TRACE.into())
        .parse_lossy(
            "info,proton_sqlite3=trace,\
                    proton_core_common=trace,proton_mail_common=trace,\
                    proton_event_loop=trace,proton_api_core=trace,\
                    proton_action_queue=trace,proton_api_mail=trace,\
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

    let keychain = InMemoryKeyChain::default();
    let key = SessionEncryptionKey::random().to_base64();
    keychain.store(key).unwrap();

    let ctx = MailContext::new(
        tmp_dir.path().join("session"),
        tmp_dir.path().join("user"),
        tmp_dir.path().join("core_cache"),
        tmp_dir.path().join("mail_cache"),
        50 * 1204 * 1024,
        Arc::new(keychain),
        Config::default(),
        None,
    )
    .await
    .unwrap();

    let mut flow = ctx.new_login_flow().unwrap();

    flow.login(username, password).await.unwrap();

    let user_ctx = ctx.user_context_from_login_flow(&mut flow).await.unwrap();

    // Sync initial data
    let cb = InitCallback;
    MailUserContext::initialize_async(user_ctx.clone(), &cb)
        .await
        .unwrap();

    let mut draft = Draft::empty(user_ctx.user_stash()).await.unwrap();

    draft.subject = subject;
    body.push_str(&draft.body);
    draft.body = body;
    draft
        .to_list
        .add_single(RecipientEntry {
            display_name: MaybeEmptyString(None),
            email: recipient,
        })
        .unwrap();

    let save_action = draft.to_save_action();
    let send_action = draft.to_send_action().unwrap();
    Draft::send(user_ctx.queue(), save_action, send_action)
        .await
        .unwrap();

    user_ctx.queue().execute_all().await.unwrap()
}
