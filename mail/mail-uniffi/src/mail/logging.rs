use chrono::Utc;
use std::fs::{self, OpenOptions};
use std::path::Path;
use tracing::error;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

/// If it finds and old log file this will add its date to it.
fn rename_old_file(log_path: &Path) -> std::io::Result<()> {
    if fs::exists(log_path)? {
        let meta = fs::metadata(log_path)?;
        let mut new = log_path.to_owned();
        let created_date = meta.created()?;
        let datetime: chrono::DateTime<Utc> = created_date.into();
        let fname = log_path.file_name().unwrap();
        let new_name = format!("{}_{fname:?}", datetime.format("%d-%m-%Y-%T"));
        new.set_file_name(new_name);
        fs::rename(log_path, new)?;
    }
    Ok(())
}

pub(super) fn init_log(log_path: &Path, debug: bool) -> std::io::Result<()> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);

    // fallback to append
    if let Err(e) = rename_old_file(log_path) {
        error!("Error renaming old log file: {e}");
        options.append(true);
    }

    let log_file = options.open(log_path)?;

    let file_subscriber = tracing_subscriber::fmt::layer()
        .with_file(false)
        .with_line_number(false)
        .with_writer(log_file)
        .with_target(false)
        .with_ansi(false)
        .with_filter(if debug {
            app_tracing_env_filter_trace()
        } else {
            app_tracing_env_filter_default()
        });
    tracing_subscriber::registry().with(file_subscriber).init();
    Ok(())
}

pub fn app_tracing_env_filter_default() -> EnvFilter {
    // TODO: once stash statistics can be disabled, remove stash=error
    EnvFilter::builder()
        .with_default_directive(LevelFilter::DEBUG.into())
        .parse(
            "info,\
            proton_mail_uniffi=debug,\
            proton_sqlite3=debug,\
            proton_core_common=debug,\
            proton_mail_common=debug,\
            proton_event_loop=debug,\
            proton_api_core=debug,\
            proton_action_queue=trace,\
            proton_api_mail=debug,\
            stash=error",
        )
        .expect("bad log directives")
}

pub fn app_tracing_env_filter_trace() -> EnvFilter {
    EnvFilter::builder()
        .with_default_directive(LevelFilter::TRACE.into())
        .parse(
            "info,\
            proton_mail_uniffi=trace,\
            proton_sqlite3=trace,\
            proton_core_common=trace,\
            proton_mail_common=trace,\
            proton_event_loop=trace,\
            proton_api_core=trace,\
            proton_action_queue=trace,\
            proton_api_mail=trace,\
            stash=trace",
        )
        .expect("bad log directives")
}
