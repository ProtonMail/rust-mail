use clap::Parser;
use proton_core_api::services::proton::LabelId;
use proton_core_common::Origin;
use proton_core_common::datatypes::ApiConfig;
use proton_core_common::db::account::SessionEncryptionKey;
use proton_core_common::event_loop::EventPollMode;
use proton_core_common::models::Label;
use proton_core_common::models::ModelIdExtension;
use proton_core_common::os::{InMemoryKeyChain, KeyChainExt};
use proton_issue_reporter_service::NoopIssueReporter;
use proton_log_service::LogService;
use proton_mail_common::MailContext;
use proton_mail_common::datatypes::ContextualConversation;
use proton_mail_common::datatypes::{ReadFilter, SystemLabelId};
use proton_mail_common::test_utils::scroller::TestScroller;
use stash::orm::Model;
use std::fmt::Debug;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::runtime;
use tracing::info;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

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
        messages: _,
    } = Args::parse();
    let tmp_dir = TempDir::new().unwrap();
    info!("TEMP_DIR = {tmp_dir:?}");

    let keychain = InMemoryKeyChain::default();
    let key = SessionEncryptionKey::random();
    keychain.store(key).unwrap();
    let config = proton_log_service::Config::builder()
        .name("log".into())
        .directory(tmp_dir.path().into())
        .build();

    let ctx = MailContext::new(
        Origin::App,
        runtime::Handle::current(),
        tmp_dir.path().join("session"),
        tmp_dir.path().join("user"),
        tmp_dir.path().join("core_cache"),
        tmp_dir.path().join("mail_cache"),
        50 * 1204 * 1024,
        Arc::new(keychain),
        ApiConfig::default(),
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

    let tether = user_ctx.user_stash().connection().await.unwrap();
    let label = Label::find_by_remote_id(LabelId::inbox(), &tether)
        .await
        .unwrap()
        .unwrap();

    let page_count = 50_u32;

    let filter = ReadFilter::Unread;
    let mut paginator =
        TestScroller::conversations(&user_ctx, label.id(), filter, page_count as usize)
            .await
            .unwrap();

    paginate_mail(&mut paginator, |v1, v2| {
        // We can only guarantee this for when no filter is applied.
        // See notes in [`MailConversationPaginatorSource`].
        // Messages don't have this issue.
        if filter != ReadFilter::All {
            return true;
        }
        // Due to a bug where attachment metadata local ids are not updated
        // during save we can not use Eq to compare both of the data items
        // as it always fails with the local id of the attachment not being present.
        v1.local_id == v2.local_id && v1.remote_id == v2.remote_id
    })
    .await;
}

async fn paginate_mail(
    paginator: &mut TestScroller<ContextualConversation>,
    is_eq: impl Fn(&ContextualConversation, &ContextualConversation) -> bool,
) {
    let mut element_count = 0_u64;
    let total_elements = paginator.total().await.unwrap();
    #[allow(clippy::cast_possible_truncation)]
    let mut all_elements = Vec::with_capacity(total_elements as usize);

    while paginator.has_more().await.unwrap() {
        let page = paginator.fetch_more_and_wait().await.unwrap();
        element_count += page.len() as u64;
        all_elements.extend(page);
        let visible = paginator.items();
        for i in 0..visible.len() {
            assert!(
                is_eq(&all_elements[i], &visible[i]),
                "Element {i} does not match"
            );
        }
        tracing::info!("Elements {} / {}", element_count, total_elements);
    }

    tracing::info!("END {} / {}", element_count, total_elements);
    assert!(element_count <= total_elements);
}
