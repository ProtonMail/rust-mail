use std::future::Future;

use proton_task_service::AsyncTaskResult;
use tokio::task::JoinHandle;

use crate::{MailContextError, MailUserContext};

mod data_scroller_source;
#[allow(clippy::wrong_self_convention)]
mod mail_scroller_state;
mod remote_source;

pub use self::data_scroller_source::*;
pub use self::remote_source::*;

pub type MailPaginatorJoinHandle =
    Option<JoinHandle<AsyncTaskResult<Result<(), MailContextError>>>>;
pub trait MailScrollerSource: Send + Sync {
    type Item: Send + 'static;

    /// Initialize the data source and retrieve up to `element_count` elements from the server.
    ///
    /// You can return an optional join handle that [`MailScroller`] will use on the first
    /// call to [`MailScroller::fetch_more()`] if you want to preload some data in
    /// a background task.
    ///
    /// # Errors
    ///
    /// Return errors if the initialization or setup failed.
    fn initialize(
        &mut self,
        ctx: &MailUserContext,
    ) -> impl Future<Output = Result<(u64, MailPaginatorJoinHandle), MailContextError>> + Send;

    /// Return the items that fall into range of the synced data.
    ///
    /// If some item is outside that range and known to us, it should not be included.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    fn visible_items(
        &self,
        ctx: &MailUserContext,
    ) -> impl Future<Output = Result<Vec<Self::Item>, MailContextError>> + Send;

    /// Return the total number of items that fall into range of the synced data.
    ///
    /// If some item is outside that range and known to us, it should not be included.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    fn visible_items_total(
        &self,
        ctx: &MailUserContext,
    ) -> impl Future<Output = Result<u64, MailContextError>> + Send;

    /// Return the total number of items that fall into range of the synced data.
    ///
    /// If some item is outside that range and known to us, it should not be included.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    fn all_items_total(
        &self,
        ctx: &MailUserContext,
    ) -> impl Future<Output = Result<u64, MailContextError>> + Send;

    /// Sync the next section of data from the remote source which should return up to
    /// `element_count` results.
    ///
    /// This method can await until the data is fetched and should return the
    /// new elements that are valid in this interval as well as the new total.
    ///
    /// # Errors
    ///
    /// Return error if something failed.
    fn sync_next(
        &mut self,
        ctx: &MailUserContext,
    ) -> impl Future<
        Output = Result<(Vec<Self::Item>, u64, MailPaginatorJoinHandle), MailContextError>,
    > + Send;

    fn watched_tables(&self) -> Vec<String>;

    fn set_notify(&mut self, _: flume::Sender<()>) {}

    /// Invalidation of the source have to be performed in both [`MailScroller`] and
    /// [`MailScrollerSource`] implementations.
    ///
    /// [`MailScroller`] will invalidate the source when it is dirty which means that
    /// when it is notified about database changes.
    ///
    /// [`MailScrollerSource`] on the other hand should invalidate itself when switching from offline
    /// to online or when new data arrives silently.
    fn invalidate(&mut self) -> impl Future<Output = Result<(), MailContextError>> + Send {
        async move { Ok(()) }
    }
}
