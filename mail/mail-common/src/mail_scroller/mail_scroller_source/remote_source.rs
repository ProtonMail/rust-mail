use std::future::Future;

use proton_api_core::services::proton::common::LabelId;
use proton_core_common::datatypes::LocalLabelId;

use crate::{datatypes::ReadFilter, models::ScrollData, MailContextError, MailUserContext};

use super::MailPaginatorJoinHandle;

mod remote_conversation_scroller_source;
mod remote_messace_scroller_source;
mod search_scroller_source;

pub use search_scroller_source::SearchScrollerSource;

pub trait RemoteSource: ScrollData + Send {
    fn sync_first_page(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
    ) -> impl Future<Output = Result<MailPaginatorJoinHandle, MailContextError>> + Send;

    fn spawn_background_sync(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        scroller: &Self,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
    ) -> impl Future<Output = Result<MailPaginatorJoinHandle, MailContextError>> + Send;
}
