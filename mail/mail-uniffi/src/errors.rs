pub(crate) mod api_service_error;
pub(crate) mod login_flow;
pub(crate) mod unexpected;

#[macro_export]
macro_rules! export_void_result {
    ($name: ident, $type:ty) => {
        #[allow(clippy::large_enum_variant)]
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
