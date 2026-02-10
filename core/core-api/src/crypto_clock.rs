#![allow(dead_code)]

use std::time::SystemTime;

use parking_lot::{Once, RwLock};
use proton_crypto_account::proton_crypto::{
    CryptoClockProvider, crypto::UnixTimestamp, crypto_clock,
};

/// Represents a clock for crypto operations that tracks time
/// via unix timestamps from the http responses.
#[derive(Debug)]
#[allow(clippy::module_name_repetitions)]
pub struct ServerCryptoClock(RwLock<Option<UnixTimestamp>>);

impl ServerCryptoClock {
    /// Updates the server clock with the observed server time.
    pub fn update_clock(&self, time: UnixTimestamp) {
        let mut cur = self.0.write();
        if let Some(current) = *cur {
            if current < time {
                *cur = Some(time);
            }
        } else {
            *cur = Some(time);
        }
    }

    fn local_unix_time() -> UnixTimestamp {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(UnixTimestamp::default(), |duration| {
                UnixTimestamp::new(duration.as_secs())
            })
    }
}

impl CryptoClockProvider for &'static ServerCryptoClock {
    fn unix_time(&self) -> UnixTimestamp {
        self.0
            .read()
            .unwrap_or(ServerCryptoClock::local_unix_time())
    }
}

impl Default for ServerCryptoClock {
    fn default() -> Self {
        Self(RwLock::new(None))
    }
}

static CRYPTO_CLOCK: ServerCryptoClock = ServerCryptoClock(RwLock::new(None));

/// Returns the global clock that tracks server time via http requests.
///
/// This clock is used for crypto operations.
#[allow(clippy::module_name_repetitions)]
#[must_use]
pub fn server_crypto_clock() -> &'static ServerCryptoClock {
    &CRYPTO_CLOCK
}

/// Set the crypto clock provider in proton crypto to the server crypto clock.
///
/// The function ensures that the provider is only initialized once.
#[allow(clippy::module_name_repetitions)]
pub fn init_server_crypto_clock() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        crypto_clock().set_provider(Box::new(&CRYPTO_CLOCK));
    });
}
