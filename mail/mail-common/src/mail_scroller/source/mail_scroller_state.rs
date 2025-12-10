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
    #[display("Online")]
    Online(CachedScrollData<T>),

    #[display("NotSynced")]
    NotSynced(CachedScrollData<T>),
}

impl<T: ScrollData> MailScrollerState<T> {
    pub async fn new_online(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        page_size: usize,
        tether: &Tether,
    ) -> Result<Self, StashError> {
        let ordered = CachedScrollData::new(local_label_id, unread, page_size, tether).await?;

        match ordered {
            Some(ordered) => Ok(MailScrollerState::Online(ordered)),
            None => {
                let order_dir = ScrollOrderDir::for_local_label(local_label_id, tether).await?;
                let order_field = ScrollOrderField::for_local_label(local_label_id, tether).await?;

                Ok(MailScrollerState::new_not_synced(
                    local_label_id,
                    unread,
                    page_size,
                    order_dir,
                    order_field,
                ))
            }
        }
    }

    pub fn new_not_synced(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        page_size: usize,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
    ) -> Self {
        let unordered =
            CachedScrollData::all(local_label_id, unread, page_size, order_dir, order_field);

        MailScrollerState::NotSynced(unordered)
    }

    pub fn online(&self) -> Option<&CachedScrollData<T>> {
        match self {
            MailScrollerState::Online(ordered) => Some(ordered),
            _ => None,
        }
    }

    pub fn not_synced(&self) -> Option<&CachedScrollData<T>> {
        match self {
            MailScrollerState::NotSynced(unordered) => Some(unordered),
            _ => None,
        }
    }

    pub fn is_online(&self) -> bool {
        matches!(self, MailScrollerState::Online { .. })
    }

    pub fn is_not_synced(&self) -> bool {
        matches!(self, MailScrollerState::NotSynced { .. })
    }

    pub async fn sync(
        &mut self,
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        page_size: usize,
        tether: &Tether,
    ) -> Result<(), StashError> {
        if let MailScrollerState::Online(ordered) = self {
            if !ordered.has_next_page(tether).await?
                && let Err(e) = ordered.update(tether).await
            {
                *self = MailScrollerState::NotSynced(ordered.clone());
                error!("Could not update scroller end cursor, it has been removed: `{e}`");
            }

            return Ok(());
        }

        let new_state =
            MailScrollerState::new_online(local_label_id, unread, page_size, tether).await?;

        if new_state.is_online() {
            *self = new_state;
        }

        Ok(())
    }

    pub async fn has_more_in_order(&self, tether: &Tether) -> Result<bool, StashError> {
        match self {
            MailScrollerState::Online(ordered) => ordered.has_more(tether).await,
            _ => Ok(false),
        }
    }

    pub async fn has_more(&self, tether: &Tether) -> Result<bool, StashError> {
        match self {
            MailScrollerState::Online(ordered) => ordered.has_more(tether).await,
            MailScrollerState::NotSynced(unordered) => unordered.has_more(tether).await,
        }
    }

    pub async fn seen_count(&self, tether: &Tether) -> Result<u64, StashError> {
        match self {
            MailScrollerState::Online(ordered) => ordered.seen_count(tether).await,
            MailScrollerState::NotSynced(unordered) => unordered.seen_count(tether).await,
        }
    }
}
