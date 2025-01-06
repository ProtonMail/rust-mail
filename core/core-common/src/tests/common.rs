#![allow(non_snake_case)]

use stash::stash::Stash;

/// Macro wrapping u64 into Option<LocalId> for easier model definition.
#[macro_export]
macro_rules! lid {
    ($id:expr) => {{
        Some($id.into())
    }};
}

/// Macro wrapping &str into Option<RemoteId> for easier model definition.
/// Since it calls .`into()` on the `RemoteId`, it allows creation of Option<LabelId> as well.
#[macro_export]
macro_rules! rid {
    ($id:expr) => {{
        use $crate::datatypes::RemoteId;
        Some(RemoteId::from($id).into())
    }};
}

#[macro_export]
macro_rules! contact {
    ($($field:tt)*) => {{
        use $crate::models::Contact;
        Contact {
            $($field)*,
            ..Default::default()
        }
    }};
}

#[macro_export]
macro_rules! contact_email {
    ($($field:tt)*) => {{
        use $crate::models::ContactEmail;
        ContactEmail {
            $($field)*,
            ..Default::default()
        }
    }};
}

pub async fn new_core_test_connection() -> Stash {
    use crate::db::migrations::migrate_core_db;
    use std::io::stdout;
    use tracing::subscriber::set_global_default;
    use tracing::Level;
    use tracing_subscriber::fmt::layer;
    use tracing_subscriber::fmt::writer::MakeWriterExt;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::{registry, EnvFilter};
    drop(set_global_default(
        registry()
            .with(EnvFilter::new("debug,stash=debug"))
            .with(layer().with_writer(stdout.with_max_level(Level::TRACE))),
    ));
    let stash = Stash::new(None).expect("Failed to create Stash");
    migrate_core_db(&stash).await.unwrap();
    stash
}
