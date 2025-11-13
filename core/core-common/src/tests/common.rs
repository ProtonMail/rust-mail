use crate::db::migrations::migrate_core_db;
use stash::stash::{Stash, StashConfiguration};
use tracing::subscriber::set_global_default;
use tracing_subscriber::fmt::layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, registry};

#[macro_export]
macro_rules! lid {
    ($id:expr) => {{ Some($id.into()) }};
}

#[macro_export]
macro_rules! cid {
    ($id:expr) => {{
        use proton_core_api::services::proton::ContactId;
        Some(ContactId::from($id))
    }};
}

#[macro_export]
macro_rules! ceid {
    ($id:expr) => {{
        use proton_core_api::services::proton::ContactEmailId;
        Some(ContactEmailId::from($id))
    }};
}

#[macro_export]
macro_rules! contact {
    ($($field:tt)*) => {{
        use $crate::models::Contact;
        Contact {
            $($field)*,
            ..Contact::test_default()
        }
    }};
}

#[macro_export]
macro_rules! contact_email {
    ($($field:tt)*) => {{
        use $crate::models::ContactEmail;
        ContactEmail {
            $($field)*,
            ..ContactEmail::test_default()
        }
    }};
}

#[macro_export]
macro_rules! label {
    ($($field:tt)*) => {{
        $crate::models::Label {
            $($field)*,
            ..Label::test_default()
        }
    }};
}

#[macro_export]
macro_rules! label_id {
    ($id:expr) => {{ proton_core_api::services::proton::LabelId::from($id) }};
}

#[macro_export]
macro_rules! labels {
    ($($label:expr),*) => {{
        $crate::datatypes::Labels::new(vec![$(
            $crate::label_id!($label)
        ),*])
    }}
}

#[macro_export]
macro_rules! device_contact {
    ($($field:tt)*) => {{
        #[allow(clippy::needless_update)] // If all fields were provided
        $crate::datatypes::DeviceContact {
            $($field)*,
            ..Default::default()
        }
    }};
}

pub async fn new_core_test_connection() -> Stash {
    _ = set_global_default(
        registry()
            .with(EnvFilter::new("debug"))
            .with(layer().with_test_writer()),
    );

    let stash = Stash::new(StashConfiguration::test()).unwrap();

    migrate_core_db(&stash).await.unwrap();

    stash
}
