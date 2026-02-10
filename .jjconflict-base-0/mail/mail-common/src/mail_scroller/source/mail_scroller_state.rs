use crate::datatypes::labels::{ScrollOrderDir, ScrollOrderField};
use crate::{
    datatypes::ReadFilter,
    models::{CachedScrollData, ScrollData},
};
use derive_more::Display;
use proton_core_common::datatypes::LocalLabelId;
use stash::stash::{StashError, Tether};
use tracing::{debug, error, info, instrument};

#[derive(Debug, Display)]
pub enum MailScrollerState<T: ScrollData> {
    #[display("Synced")]
    Synced(CachedScrollData<T>),

    #[display("Unsynced")]
    Unsynced(CachedScrollData<T>),
}

impl<T: ScrollData> MailScrollerState<T> {
    #[instrument(skip(tether))]
    pub async fn synced(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        page_size: usize,
        tether: &Tether,
    ) -> Result<Self, StashError> {
        info!("Creating new synced state");

        let state = CachedScrollData::new(local_label_id, unread, page_size, tether).await?;

        match state {
            Some(state) => Ok(MailScrollerState::Synced(state)),

            None => {
                let order_dir = ScrollOrderDir::for_local_label(local_label_id, tether).await?;
                let order_field = ScrollOrderField::for_local_label(local_label_id, tether).await?;

                Ok(MailScrollerState::unsynced(
                    local_label_id,
                    unread,
                    page_size,
                    order_dir,
                    order_field,
                ))
            }
        }
    }

    #[instrument]
    pub fn unsynced(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        page_size: usize,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
    ) -> Self {
        info!("Creating new unsynced state");

        let state =
            CachedScrollData::all(local_label_id, unread, page_size, order_dir, order_field);

        MailScrollerState::Unsynced(state)
    }

    pub fn as_synced(&self) -> Option<&CachedScrollData<T>> {
        match self {
            MailScrollerState::Synced(ordered) => Some(ordered),
            _ => None,
        }
    }

    pub fn is_synced(&self) -> bool {
        matches!(self, MailScrollerState::Synced { .. })
    }

    pub fn is_unsynced(&self) -> bool {
        matches!(self, MailScrollerState::Unsynced { .. })
    }

    #[instrument(skip_all)]
    pub async fn sync(
        &mut self,
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        page_size: usize,
        tether: &Tether,
    ) -> Result<(), StashError> {
        debug!("Synchronizing state");

        if let MailScrollerState::Synced(ordered) = self {
            if !ordered.has_next_page(tether).await?
                && let Err(e) = ordered.update(tether).await
            {
                error!("Could not update scroller end cursor, it has been removed: `{e}`");
                *self = MailScrollerState::Unsynced(ordered.clone());
            }

            return Ok(());
        }

        let new_state =
            MailScrollerState::synced(local_label_id, unread, page_size, tether).await?;

        if new_state.is_synced() {
            *self = new_state;
        }

        Ok(())
    }

    pub async fn has_more(&self, tether: &Tether) -> Result<bool, StashError> {
        match self {
            MailScrollerState::Synced(state) => state.has_more(tether).await,
            MailScrollerState::Unsynced(state) => state.has_more(tether).await,
        }
    }

    pub async fn has_more_synced(&self, tether: &Tether) -> Result<bool, StashError> {
        match self {
            MailScrollerState::Synced(state) => state.has_more(tether).await,
            _ => Ok(false),
        }
    }

    pub async fn seen_count(&self, tether: &Tether) -> Result<u64, StashError> {
        match self {
            MailScrollerState::Synced(state) => state.seen_count(tether).await,
            MailScrollerState::Unsynced(state) => state.seen_count(tether).await,
        }
    }
}
