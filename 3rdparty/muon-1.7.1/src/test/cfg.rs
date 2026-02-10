#[doc(hidden)]
#[macro_export]
macro_rules! if_crypto {
    ($($tt:tt)*) => {
        if_not_wasm! {
            if_cfg! { #[cfg(feature = "testing-crypto")], $($tt)* }
        }
    }
}

#[doc(hidden)]
#[macro_export]
macro_rules! if_tinyproxy {
    ($($tt:tt)*) => {
        if_not_wasm! {
            if_cfg! { #[cfg(feature = "testing-tinyproxy")], $($tt)* }
        }
    }
}

#[doc(hidden)]
#[macro_export]
macro_rules! if_server {
    ($($tt:tt)*) => {
        if_not_wasm! {
            if_cfg! { #[cfg(feature = "testing-server")], $($tt)* }
        }
    }
}

#[doc(hidden)]
#[macro_export]
macro_rules! if_runner {
    ($($tt:tt)*) => {
        if_not_wasm! {
            if_cfg! { #[cfg(feature = "testing-runner")], $($tt)* }
        }
    }
}
