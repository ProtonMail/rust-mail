use anyhow::Result;
use clap::Parser;
use proton_core_api::session::EnvId;
use proton_core_common::Origin;
use proton_core_common::datatypes::ApiConfig;
use proton_core_common::db::account::SessionEncryptionKey;
use proton_core_common::event_loop::EventPollMode;
use proton_core_common::os::{InMemoryKeyChain, KeyChainExt};
use proton_log_service::LogService;
use proton_mail_common::MailContext;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

#[derive(Parser)]
#[command(name = "feature_flags")]
#[command(about = "Unleash Feature Flags CLI Example")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    List,
    Check { name: String },
    Refresh,
}

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging()?;

    let cli = Cli::parse();
    let ctx = create_mail_context().await?;

    sleep(Duration::from_millis(500)).await;

    match cli.command {
        Commands::List => list_feature_flags(&ctx).await?,
        Commands::Check { name } => check_feature_flag(&ctx, &name).await?,
        Commands::Refresh => refresh_feature_flags(&ctx).await?,
    }

    Ok(())
}

async fn create_mail_context() -> Result<Arc<MailContext>> {
    let temp_dir = TempDir::new()?;
    let temp_path = temp_dir.path();

    let session_path = temp_path.join("session");
    let user_path = temp_path.join("user");
    let core_cache_path = temp_path.join("core_cache");
    let mail_cache_path = temp_path.join("mail_cache");

    std::fs::create_dir_all(&session_path)?;
    std::fs::create_dir_all(&user_path)?;
    std::fs::create_dir_all(&core_cache_path)?;
    std::fs::create_dir_all(&mail_cache_path)?;

    let keychain = InMemoryKeyChain::default();
    let key = SessionEncryptionKey::random();
    keychain.store(key)?;

    let config = proton_log_service::Config::builder()
        .name("log".into())
        .directory(temp_path.into())
        .build();

    let ctx = MailContext::new(
        Origin::App,
        tokio::runtime::Handle::current(),
        session_path,
        user_path,
        core_cache_path,
        mail_cache_path,
        50 * 1024 * 1024,
        Arc::new(keychain),
        ApiConfig::default_with_env(EnvId::new_atlas()),
        None,
        None,
        LogService::new(config),
        EventPollMode::Manual,
        Default::default(),
    )
    .await?;

    Ok(ctx)
}

async fn list_feature_flags(ctx: &Arc<MailContext>) -> Result<()> {
    let service = ctx.feature_flags();
    let flags = service.list_all().await;

    if flags.is_empty() {
        warn!("No feature flags found");
        warn!("Try: cargo run --example feature_flags -- refresh");
    } else {
        info!("Found {} feature flags:", flags.len());
        for (name, enabled) in flags {
            let status = if enabled { "🟢" } else { "🔴" };
            info!("  {} {}", status, name);
        }
    }
    Ok(())
}

async fn check_feature_flag(ctx: &Arc<MailContext>, flag_name: &str) -> Result<()> {
    let service = ctx.feature_flags();
    match service.get(flag_name).await {
        Some(true) => info!("✅ {} is ENABLED", flag_name),
        Some(false) => info!("❌ {} is DISABLED", flag_name),
        None => warn!("❓ {} is UNKNOWN", flag_name),
    }
    Ok(())
}

async fn refresh_feature_flags(ctx: &Arc<MailContext>) -> Result<()> {
    let service = ctx.feature_flags();
    match service.refresh().await {
        Ok(()) => info!("✅ Feature flags refreshed successfully"),
        Err(e) => error!("❌ Refresh failed: {}", e),
    }
    Ok(())
}

fn setup_logging() -> Result<()> {
    let env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();

    tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter(env_filter)
        .init();

    Ok(())
}
