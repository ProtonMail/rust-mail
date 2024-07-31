use anyhow::anyhow;
use proton_mail_common::db::proton_sqlite3::rusqlite::Error;

#[macro_export]
macro_rules! new_live_query {
    ($name:ident, $query:ident) => {
        /// Live queries behave similarly to CoreData/Room's FetchedResult/ObservedQueries. However, since
        /// the observation happens from the rust side we can't provide optimal default integration in
        /// the target application runtime (JetPack Compose/SwiftUI).
        ///
        /// Live queries accept a callback which will be triggered when the query has been refreshed.
        /// Refresh can occur when the tables the query is watching are modified.
        /// Once a callback has occurred you should [`$name::value()`] to access the new data.
        ///
        /// [`$name::value()`] can be called as many times as you wish and will always return the
        /// latest result of the query.
        ///
        #[derive(uniffi::Object)]
        pub struct $name(SharedLive<$query>);

        #[uniffi::export]
        impl $name {
            /// Get the latest value for this Query.
            pub fn value(
                &self,
            ) -> Result<<$query as Observable>::Output, $crate::macros::LiveQueryError> {
                use std::ops::Deref;
                let value = self.0.value();
                match value.deref() {
                    Ok(v) => Ok(v.clone()),
                    Err(e) => Err($crate::macros::LiveQueryError::from_error(e)),
                }
            }

            /// Terminate the observer for this query and stop receiving updates.
            pub fn disconnect(&self) {
                self.0.disconnect();
            }
        }

        impl $name {
            #[allow(unused)]
            fn new(
                tracker: InProcessTrackerService,
                query: $query,
                cb: Box<dyn MailboxLiveQueryUpdatedCallback>,
            ) -> Arc<Self> {
                Arc::new(Self(
                    SharedLiveQueryBuilder::new(tracker)
                        .with_background_initializer()
                        .with_callback(cb)
                        .build(query),
                ))
            }

            #[allow(unused)]
            fn new_foreground(
                tracker: InProcessTrackerService,
                query: $query,
                cb: Box<dyn MailboxLiveQueryUpdatedCallback>,
            ) -> Arc<Self> {
                Arc::new(Self(
                    SharedLiveQueryBuilder::new(tracker)
                        .with_foreground_initializer()
                        .with_callback(cb)
                        .build(query),
                ))
            }
        }
    };
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum LiveQueryError {
    #[error("Observed resource not found")]
    DataNotFound,
    #[error("{0}")]
    Error(anyhow::Error),
}

impl LiveQueryError {
    pub fn from_error(value: &Error) -> Self {
        match value {
            Error::QueryReturnedNoRows => Self::DataNotFound,
            _ => Self::Error(anyhow!("{value}")),
        }
    }
}
