//! Collection of functions to allow mobile to log with the rust log system.

use proton_mail_common::exports::tracing;

/// Log `line` with info level.
#[uniffi::export]
pub fn rust_log_info(line: &str) {
    tracing::info!("{line}");
}

/// Log `line` with debug level.
#[uniffi::export]
pub fn rust_log_debug(line: &str) {
    tracing::debug!("{line}");
}

/// Log `line` with trace level.
#[uniffi::export]
pub fn rust_log_trace(line: &str) {
    tracing::trace!("{line}");
}

/// Log `line` with warning level.
#[uniffi::export]
pub fn rust_log_warn(line: &str) {
    tracing::warn!("{line}");
}

/// Log `line` with error level.
#[uniffi::export]
pub fn rust_log_error(line: &str) {
    tracing::error!("{line}");
}
