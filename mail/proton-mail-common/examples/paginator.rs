use clap::Parser;
use proton_api_core::services::proton::Config;
use proton_api_core::session::CoreSession;
use proton_core_common::datatypes::LabelId;
use proton_core_common::db::session::SessionEncryptionKey;
use proton_core_common::os::{InMemoryKeyChain, KeyChain};
use proton_mail_common::datatypes::{ContextualConversation, SystemLabelId};
use proton_mail_common::models::{Conversation, Message};
use proton_mail_common::{
    MailContext, MailContextError, MailUserContextInitializationCallback,
    MailUserContextLoadingStage, Mailbox,
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

    let Args { username, password } = Args::parse();
    let tmp_dir = TempDir::new("cli").unwrap();

    let keychain = InMemoryKeyChain::default();
    let key = SessionEncryptionKey::random().to_base64();
    keychain.store(key).unwrap();

    let ctx = MailContext::new(
        tmp_dir.path().join("session"),
        tmp_dir.path().join("user"),
        tmp_dir.path().join("mail_cache"),
        50 * 1204 * 1024,
        Arc::new(keychain),
        Config::default(),
        None,
    )
    .await
    .unwrap();

    let mut flow = ctx.new_login_flow().await.unwrap();

    flow.login(username, password, None).await.unwrap();

    let user_ctx = ctx.user_context_from_login_flow(&flow).await.unwrap();

    // Sync initial data
    let cb = InitCallback;
    user_ctx.initialize_async(&cb).await.unwrap();

    let mailbox = Mailbox::with_remote_id(Arc::clone(&user_ctx), LabelId::inbox())
        .await
        .unwrap();

    let page_count = 5_u32;

    let paginator =
        Conversation::paginate_in_label(&user_ctx, mailbox.label_id(), page_count, None)
            .await
            .unwrap();
    // Uncomment for messages.
    /*
    let paginator = Message::paginate_in_label(
        &user_ctx,
        mailbox.label_id(),
        page_count,
        user_ctx.user_stash(),
        None,
    )
    .await
    .unwrap();*/

    let page_1 = paginator.current_page().await.unwrap();
    assert_eq!(page_1.len(), page_count as usize);
    let page_2 = paginator.next_page().await.unwrap();
    assert_eq!(page_2.len(), page_count as usize);
    assert_ne!(page_1.last().unwrap(), page_2.first().unwrap());
}
