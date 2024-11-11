pub(crate) mod api_service_error;
pub(crate) mod login_flow;
mod proton_mail_error;
pub(crate) mod unexpected;
pub(crate) mod user_session;

pub use self::proton_mail_error::*;

#[macro_export]
macro_rules! export_void_result {
    ($name: ident, $type:ty) => {
        #[allow(clippy::large_enum_variant)]
        #[allow(dead_code)]
        #[derive(uniffi::Object)]
        pub enum $name {
            Ok,
            Error($type),
        }

        impl<T, E: Into<$type>> From<Result<T, E>> for $name {
            fn from(value: Result<T, E>) -> Self {
                match value {
                    Ok(_) => Self::Ok,
                    Err(error) => Self::Error(error.into()),
                }
            }
        }

        impl<E: Into<$type>> From<E> for $name {
            fn from(error: E) -> Self {
                Self::Error(error.into())
            }
        }
    };
}

#[macro_export]
macro_rules! export_typed_result {
    ($name: ident, $ok_type: ty, $err_type: ty) => {
        #[allow(clippy::large_enum_variant)]
        #[allow(dead_code)]
        #[derive(uniffi::Object)]
        pub enum $name {
            Ok($ok_type),
            Error($err_type),
        }

        impl<T: Into<$ok_type>, E: Into<$err_type>> From<Result<T, E>> for $name {
            fn from(value: Result<T, E>) -> Self {
                match value {
                    Ok(val) => Self::Ok(val.into()),
                    Err(error) => Self::Error(error.into()),
                }
            }
        }
    };
}
