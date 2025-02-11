//! Collection of functions to allow mobile to log with the rust log system.

use tracing::{debug, error, info, trace, warn};

/// Log `line` with info level.
#[uniffi_export]
pub fn rust_log_info(line: &str) {
    info!("{line}");
}

/// Log `line` with debug level.
#[uniffi_export]
pub fn rust_log_debug(line: &str) {
    debug!("{line}");
}

/// Log `line` with trace level.
#[uniffi_export]
pub fn rust_log_trace(line: &str) {
    trace!("{line}");
}

/// Log `line` with warning level.
#[uniffi_export]
pub fn rust_log_warn(line: &str) {
    warn!("{line}");
}

/// Log `line` with error level.
#[uniffi_export]
pub fn rust_log_error(line: &str) {
    error!("{line}");
}
