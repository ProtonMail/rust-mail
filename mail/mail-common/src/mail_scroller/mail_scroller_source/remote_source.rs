use std::future::Future;

use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::LocalLabelId;

use crate::{MailContextError, MailUserContext, datatypes::ReadFilter, models::ScrollData};

use super::MailPaginatorJoinHandle;

mod remote_conversation_scroller_source;
mod remote_messace_scroller_source;
mod search_scroller_source;

use crate::datatypes::labels::LabelScrollOrder;
pub use search_scroller_source::SearchScrollerSource;

pub trait RemoteSource: ScrollData + Send + Sync {
    fn sync_first_page(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
        label_scroll_order: LabelScrollOrder,
    ) -> impl Future<Output = Result<MailPaginatorJoinHandle, MailContextError>> + Send;

    fn sync_next_page(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        scroller: &Self,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
        label_scroll_order: LabelScrollOrder,
    ) -> impl Future<Output = Result<MailPaginatorJoinHandle, MailContextError>> + Send;

    #[allow(clippy::too_many_arguments)]
    fn sync_previous_page(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        scroller: &Self,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
        label_scroll_order: LabelScrollOrder,
        callback: flume::Sender<()>,
    ) -> impl Future<Output = Result<MailPaginatorJoinHandle, MailContextError>> + Send;
}
