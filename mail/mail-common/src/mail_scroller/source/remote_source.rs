mod conversations;
mod messages;
mod search;

pub use self::search::*;
use super::MailPaginatorJoinHandle;
use crate::datatypes::labels::ScrollOrderDir;
use crate::{
    MailContextError, MailUserContext,
    datatypes::{ReadFilter, labels::ScrollOrderField},
    models::ScrollData,
};
use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::LocalLabelId;

pub trait RemoteSource: ScrollData + Send + Sync {
    #[allow(clippy::too_many_arguments)]
    fn sync_first_page(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
        invalidate: Option<flume::Sender<()>>,
    ) -> Result<MailPaginatorJoinHandle, MailContextError>;

    #[allow(clippy::too_many_arguments)]
    fn sync_next_page(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        scroller: &Self,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
    ) -> Result<MailPaginatorJoinHandle, MailContextError>;

    #[allow(clippy::too_many_arguments)]
    fn sync_previous_page(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        scroller: &Self,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
        callback: Option<flume::Sender<()>>,
    ) -> Result<MailPaginatorJoinHandle, MailContextError>;
}
