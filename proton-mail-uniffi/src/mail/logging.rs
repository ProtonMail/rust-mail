use proton_mail_common::exports::tracing::level_filters::LevelFilter;
use std::path::Path;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

pub(super) fn init_log(log_path: &Path, debug: bool) -> std::io::Result<()> {
    let log_file = std::fs::File::create(log_path)?;
    let file_subscriber = tracing_subscriber::fmt::layer()
        .with_file(false)
        .with_line_number(false)
        .with_writer(log_file)
        .with_target(false)
        .with_ansi(false)
        .with_filter(if debug {
            app_tracing_env_filter_debug()
        } else {
            app_tracing_env_filter_default()
        });
    tracing_subscriber::registry().with(file_subscriber).init();
    Ok(())
}

pub fn app_tracing_env_filter_default() -> EnvFilter {
    EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .parse_lossy("info")
}

pub fn app_tracing_env_filter_debug() -> EnvFilter {
    EnvFilter::builder()
        .with_default_directive(LevelFilter::TRACE.into())
        .parse_lossy(
            "info,proton_mail_uniffi=debug,proton_mail_db=trace,proton_sqlite3=trace,\
                    proton_core_db=trace,proton_core_common=trace,proton_mail_common=trace,\
                    proton_event_loop=trace,proton_api_core=trace",
        )
}
