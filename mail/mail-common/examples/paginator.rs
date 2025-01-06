use clap::Parser;
use proton_api_core::services::proton::common::LabelId;
use proton_api_core::session::Config;
use proton_core_common::db::account::SessionEncryptionKey;
use proton_core_common::models::ModelIdExtension;
use proton_core_common::os::{InMemoryKeyChain, KeyChain};
use proton_core_common::paginator::DataSource;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::models::{
    Conversation, Label, Message, PaginatorCompat, PaginatorFilter, PaginatorSearchOptions,
};
use proton_mail_common::{
    MailContext, MailContextError, MailUserContext, MailUserContextInitializationCallback,
    MailUserContextLoadingStage,
};
use stash::orm::Model;
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
    #[arg(short, long, default_value = "false")]
    messages: bool,
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
        messages,
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

    let tether = user_ctx.user_stash().connection();
    let label = Label::find_by_remote_id(LabelId::inbox(), &tether)
        .await
        .unwrap()
        .unwrap();

    let page_count = 50_u32;

    if messages {
        let paginator = Message::paginate_in_label(
            &user_ctx,
            label.local_id.unwrap(),
            page_count,
            PaginatorFilter::default(),
            PaginatorSearchOptions::default(),
            true,
        )
        .await
        .unwrap();
        paginate(&paginator, label.total_msg).await;
    } else {
        let paginator = Conversation::paginate_in_label(
            &user_ctx,
            label.local_id.unwrap(),
            page_count,
            PaginatorFilter::default(),
            true,
        )
        .await
        .unwrap();
        paginate(&paginator, label.total_conv).await;
    }
}

async fn paginate<T: Model, R: DataSource<Item = T>>(
    paginator: &PaginatorCompat<T, R>,
    total_elements: u64,
) {
    let mut element_count = 0_u64;

    while paginator.has_next_page().await {
        let page = paginator.next_page().await.unwrap();
        tracing::info!("Elements {} / {}", element_count, total_elements);
        element_count += page.len() as u64;
    }

    tracing::info!("END {} / {}", element_count, total_elements);
}
