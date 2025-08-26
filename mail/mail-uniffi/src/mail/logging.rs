use proton_log_service::LogService;
use std::backtrace::Backtrace;
use std::panic::{set_hook, take_hook};
use tracing::error;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

pub(super) fn init_log(log_service: &LogService, debug: bool) -> std::io::Result<()> {
    let file_subscriber = log_service
        .create_layer()?
        .with_file(false)
        .with_line_number(false)
        .with_target(false)
        .with_ansi(false)
        .with_filter(if debug {
            app_tracing_env_filter_trace()
        } else {
            app_tracing_env_filter_default()
        });

    #[cfg(target_os = "ios")]
    let os_log_subscriber =
        tracing_oslog::OsLogger::new("ch.protonmail.protonmail", "[Proton] Rust").with_filter(
            if debug {
                app_tracing_env_filter_trace()
            } else {
                app_tracing_env_filter_default()
            },
        );

    let registry = tracing_subscriber::registry().with(file_subscriber);

    #[cfg(target_os = "ios")]
    let registry = { registry.with(os_log_subscriber) };

    if let Err(e) = registry.try_init() {
        tracing::warn!("Failed to initialize logging: {e}");
        eprintln!("Failed to initialize logging: {e}");
    }

    tracing::info!(path=?log_service.default_log_path(), "Path to log");

    log_backtrace_on_panic();
    Ok(())
}

pub fn app_tracing_env_filter_default() -> EnvFilter {
    EnvFilter::builder()
        .with_default_directive(LevelFilter::DEBUG.into())
        .parse(format!(
            "info,\
            {},\
            proton_mail_uniffi=debug,\
            proton_sqlite3=debug,\
            proton_calendar_common=debug,\
            proton_core_common=debug,\
            proton_mail_common=debug,\
            proton_event_loop=debug,\
            proton_core_api=debug,\
            proton_action_queue=trace,\
            proton_mail_api=debug,\
            stash={}",
            LogService::silence_muon_errors_evn_filter(),
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
            proton_calendar_common=trace,\
            proton_core_common=trace,\
            proton_mail_common=trace,\
            proton_event_loop=trace,\
            proton_core_api=trace,\
            proton_action_queue=trace,\
            proton_mail_api=trace,\
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
