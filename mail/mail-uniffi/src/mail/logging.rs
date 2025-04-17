use std::backtrace::Backtrace;
use std::fs::OpenOptions;
use std::panic::{set_hook, take_hook};
use std::path::Path;
use tracing::error;
use tracing_appender::non_blocking;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

#[cfg(target_os = "ios")]
pub(super) fn init_log(log_path: &Path, debug: bool) -> std::io::Result<Option<WorkerGuard>> {
    let log_file = OpenOptions::new()
        .read(true)
        .create(true)
        .append(true)
        .open(log_path)?;

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
    log_backtrace_on_panic();
    Ok(None)
}

#[cfg(not(target_os = "ios"))]
pub(super) fn init_log(log_path: &Path, debug: bool) -> std::io::Result<Option<WorkerGuard>> {
    let log_file = OpenOptions::new()
        .read(true)
        .create(true)
        .append(true)
        .open(log_path)?;

    let (appender, guard) = non_blocking(log_file);

    let file_subscriber = tracing_subscriber::fmt::layer()
        .with_file(false)
        .with_line_number(false)
        .with_writer(appender)
        .with_target(false)
        .with_ansi(false)
        .with_filter(if debug {
            app_tracing_env_filter_trace()
        } else {
            app_tracing_env_filter_default()
        });
    tracing_subscriber::registry().with(file_subscriber).init();
    log_backtrace_on_panic();
    Ok(Some(guard))
}

pub fn app_tracing_env_filter_default() -> EnvFilter {
    EnvFilter::builder()
        .with_default_directive(LevelFilter::DEBUG.into())
        .parse(format!(
            "info,\
            muon=debug,\
            muon_impl=debug,\
            proton_mail_uniffi=debug,\
            proton_sqlite3=debug,\
            proton_core_common=debug,\
            proton_mail_common=debug,\
            proton_event_loop=debug,\
            proton_api_core=debug,\
            proton_action_queue=trace,\
            proton_api_mail=debug,\
            stash={}",
            if std::env::var("STASH_SQL_DEBUG").is_ok() {
                "trace"
            } else {
                "error"
            }
        ))
        .expect("bad log directives")
}

pub fn app_tracing_env_filter_trace() -> EnvFilter {
    EnvFilter::builder()
        .with_default_directive(LevelFilter::TRACE.into())
        .parse(format!(
            "info,\
            muon=trace,\
            muon_impl=trace,\
            proton_mail_uniffi=trace,\
            proton_sqlite3=trace,\
            proton_core_common=trace,\
            proton_mail_common=trace,\
            proton_event_loop=trace,\
            proton_api_core=trace,\
            proton_action_queue=trace,\
            proton_api_mail=trace,\
            stash={}",
            if std::env::var("STASH_SQL_DEBUG").is_ok() {
                "trace"
            } else {
                "error"
            }
        ))
        .expect("bad log directives")
}

/// Modify the hook on panic so we log the `Backtrace`.
fn log_backtrace_on_panic() {
    let previous_hook = take_hook();
    set_hook(Box::new(move |info| {
        error!("Backtrace: {info}\n{}", Backtrace::force_capture());
        previous_hook(info);
    }));
}
