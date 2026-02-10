#[doc(hidden)]
#[macro_export]
macro_rules! if_rt_async {
    ($($tt:tt)*) => {
        $crate::if_cfg! { #[cfg(feature = "rt-async")], $($tt)* }
    }
}

#[doc(hidden)]
#[macro_export]
macro_rules! if_rt_tokio {
    ($($tt:tt)*) => {
        $crate::if_cfg! { #[cfg(feature = "rt-tokio")], $($tt)* }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! if_rt {
    ($($tt:tt)*) => {
        $crate::if_cfg! { #[cfg(any(feature = "rt-async", feature = "rt-tokio"))], $($tt)* }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! if_tls_rustls {
    ($($tt:tt)*) => {
        $crate::if_cfg! { #[cfg(feature = "tls-rustls")], $($tt)* }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! if_tls_tokio {
    ($($tt:tt)*) => {
        $crate::if_cfg! { #[cfg(any(feature = "tls-tokio-rustls", feature = "tls-tokio-native",))], $($tt)* }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! if_tls_pinning {
    ($($tt:tt)*) => {
        $crate::if_cfg! { #[cfg(feature = "tls-pinning")], $($tt)* }
    }
}

#[doc(hidden)]
#[macro_export]
macro_rules! if_tls {
    ($($tt:tt)*) => {
        $crate::if_cfg! { #[cfg(any(feature = "tls-rustls", feature = "tls-tokio"))], $($tt)* }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! if_dns_client {
    ($($tt:tt)*) => {
        $crate::if_cfg! { #[cfg(feature = "dns-client")], $($tt)* }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! if_doh_client {
    ($($tt:tt)*) => {
        $crate::if_cfg! { #[cfg(feature = "doh-client")], $($tt)* }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! if_dns {
    ($($tt:tt)*) => {
        $crate::if_cfg! { #[cfg(any(feature = "dns-client", feature = "doh-client"))], $($tt)* }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! if_http_proxy {
    ($($tt:tt)*) => {
        $crate::if_cfg! { #[cfg(feature = "http-proxy")], $($tt)* }
    }
}

#[doc(hidden)]
#[macro_export]
macro_rules! if_unsealed {
    ($($tt:tt)*) => {
        $crate::if_cfg! { #[cfg(feature = "unsealed")], $($tt)* }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! if_sealed {
    ($($tt:tt)*) => {
        $crate::if_cfg! { #[cfg(not(feature = "unsealed"))], $($tt)* }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! if_wasm {
    ($($tt:tt)*) => {
        $crate::if_cfg! { #[cfg(target_family = "wasm")], $($tt)* }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! if_not_wasm {
    ($($tt:tt)*) => {
        $crate::if_cfg! { #[cfg(not(target_family = "wasm"))], $($tt)* }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! if_android {
    ($($tt:tt)*) => {
        $crate::if_cfg! { #[cfg(target_os = "android")], $($tt)* }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! if_cfg {
    (#[$($mm:tt)*], $tt:tt else $($ff:tt)*) => {
        $crate::deps::cfg_if::cfg_if! {
            if #[$($mm)*] {
                $crate::__unwrap! { $tt }
            } else {
                $crate::__unwrap! { $($ff)* }
            }
        }
    };

    (#[$($mm:tt)*], $($tt:tt)*) => {
        $crate::deps::cfg_if::cfg_if! {
            if #[$($mm)*] {
                $crate::__unwrap! { $($tt)* }
            }
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __unwrap {
    ({ $($tt:tt)* }) => {
        $crate::__unwrap! { $($tt)* }
    };

    ($($tt:tt)*) => {
        $($tt)*
    };
}
