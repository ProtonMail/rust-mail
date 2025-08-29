use derive_more::Display;
use proton_core_common::datatypes::LocalLabelId;
use stash::stash::{StashError, Tether};

use crate::datatypes::labels::{ScrollOrderDir, ScrollOrderField};
use crate::{
    datatypes::ReadFilter,
    models::{CachedScrollData, ScrollData},
};

/// Keeps track of the state of the CachedScrollData.
///
/// This is used to differentiate between online, offline, not synced and none states.
/// Which can dynamically change based on the remote availability of the data.
///
#[derive(Debug, Display)]
pub enum MailScrollerState<T: ScrollData> {
    /// The data is source is ordered and synced with the server.
    #[display("Online")]
    Online(CachedScrollData<T>),

    /// Partially synced data, where the ordered data is synced with the server.
    /// But server is not available so we have to rely on the unordered data.
    #[display("Offline")]
    Offline {
        ordered: CachedScrollData<T>,
        unordered: CachedScrollData<T>,
    },

    /// The data is not synced with the server. This is used when the server is not available.
    /// And we have to rely on the unordered data.
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

    pub fn to_online(&mut self) {
        if let MailScrollerState::Offline { ordered, .. } = self {
            *self = MailScrollerState::Online(ordered.clone());
        }
    }

    pub fn offline(&self) -> Option<&CachedScrollData<T>> {
        match self {
            MailScrollerState::Offline { unordered, .. } => Some(unordered),
            MailScrollerState::NotSynced(unordered) => Some(unordered),
            _ => None,
        }
    }

    pub fn to_offline(&mut self) {
        if let MailScrollerState::Online(ordered) = self {
            let ordered = ordered.clone();
            let unordered = ordered.clone().set_absolute_end();
            *self = MailScrollerState::Offline { ordered, unordered };
        }
    }

    pub fn is_online(&self) -> bool {
        matches!(self, MailScrollerState::Online { .. })
    }

    pub fn is_offline(&self) -> bool {
        matches!(self, MailScrollerState::Offline { .. })
            || matches!(self, MailScrollerState::NotSynced { .. })
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
        match self {
            MailScrollerState::Online(ordered) | MailScrollerState::Offline { ordered, .. } => {
                if !ordered.has_next_page(tether).await?
                    && let Err(e) = ordered.update(tether).await
                {
                    tracing::error!(
                        "Could not update scroller end cursor, it has been removed: `{e}`"
                    );
                    *self = MailScrollerState::NotSynced(ordered.clone());
                }

                return Ok(());
            }
            _ => {}
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
            MailScrollerState::Offline { ordered, .. } => ordered.has_more(tether).await,
            _ => Ok(false),
        }
    }
}
