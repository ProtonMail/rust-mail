/// Very simple ping tests.
mod ping;

/// Auth (SRP) tests.
mod auth;

/// Request building and sending tests.
mod request;

/// Retry handling tests.
mod retries;

/// Timeout tests.
mod timeout;

/// Runtime tests.
mod runtime;

/// TLS tests.
mod tls;

/// Drop tests.
mod drop;

/// Proxy tests.
#[cfg(feature = "test-proxy")]
mod proxy;
