use proton_core_common::datatypes::LocalLabelId;
use stash::stash::{StashError, Tether};

use crate::datatypes::labels::LabelScrollOrder;
use crate::{
    datatypes::ReadFilter,
    models::{CachedScrollData, ScrollData},
};

/// Keeps track of the state of the CachedScrollData.
///
/// This is used to differentiate between online, offline, not synced and none states.
/// Which can dynamically change based on the remote availability of the data.
///
#[derive(Debug)]
pub enum MailScrollerState<T: ScrollData> {
    /// The data is source is ordered and synced with the server.
    Online(CachedScrollData<T>),

    /// Partially synced data, where the ordered data is synced with the server.
    /// But server is not available so we have to rely on the unordered data.
    Offline {
        ordered: CachedScrollData<T>,
        unordered: CachedScrollData<T>,
    },

    /// The data is not synced with the server. This is used when the server is not available.
    /// And we have to rely on the unordered data.
    NotSynced(CachedScrollData<T>),

    /// The data is not available and/or is not yet initialized
    None,
}

impl<T: ScrollData> MailScrollerState<T> {
    /// Try create new online state.
    pub async fn new_online(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        page_size: usize,
        tether: &Tether,
    ) -> Result<Self, StashError> {
        let ordered = CachedScrollData::new(local_label_id, unread, page_size, tether).await?;

        match ordered {
            Some(ordered) => Ok(MailScrollerState::Online(ordered)),
            None => Ok(MailScrollerState::None),
        }
    }

    /// Create new not synced state.
    pub fn new_not_synced(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        page_size: usize,
        scroll_order: LabelScrollOrder,
    ) -> Self {
        let unordered = CachedScrollData::all(local_label_id, unread, page_size, scroll_order);

        MailScrollerState::NotSynced(unordered)
    }

    /// If state is online, return the ordered data.
    pub fn online(&self) -> Option<&CachedScrollData<T>> {
        match self {
            MailScrollerState::Online(ordered) => Some(ordered),
            _ => None,
        }
    }

    /// Try to switch from offline to online state.
    pub fn to_online(&mut self) {
        if let MailScrollerState::Offline { ordered, .. } = self {
            *self = MailScrollerState::Online(ordered.clone());
        }
    }

    /// If state is offline, return the unordered data.
    pub fn offline(&self) -> Option<&CachedScrollData<T>> {
        match self {
            MailScrollerState::Offline { unordered, .. } => Some(unordered),
            MailScrollerState::NotSynced(unordered) => Some(unordered),
            _ => None,
        }
    }

    /// If state is offline, return the mutable reference to the unordered data.
    pub fn offline_mut(&mut self) -> Option<&mut CachedScrollData<T>> {
        match self {
            MailScrollerState::Offline { unordered, .. } => Some(unordered),
            MailScrollerState::NotSynced(unordered) => Some(unordered),
            _ => None,
        }
    }

    /// Try to switch from online to offline state.
    pub fn to_offline(&mut self) {
        if let MailScrollerState::Online(ordered) = self {
            let ordered = ordered.clone();
            let unordered = ordered.clone().set_absolute_end();
            *self = MailScrollerState::Offline { ordered, unordered };
        }
    }

    /// Check if the state is online
    pub fn is_online(&self) -> bool {
        matches!(self, MailScrollerState::Online { .. })
    }

    /// Check if the state is offline
    pub fn is_offline(&self) -> bool {
        matches!(self, MailScrollerState::Offline { .. })
            || matches!(self, MailScrollerState::NotSynced { .. })
    }

    pub fn is_not_synced(&self) -> bool {
        matches!(self, MailScrollerState::NotSynced { .. })
    }

    /// Check if the state is none
    pub fn is_none(&self) -> bool {
        matches!(self, MailScrollerState::None)
    }

    /// Try to sync ordered data end cursor.
    pub async fn sync(
        &mut self,
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        page_size: usize,
        tether: &Tether,
    ) -> Result<(), StashError> {
        match self {
            MailScrollerState::Online(ordered) | MailScrollerState::Offline { ordered, .. } => {
                if !ordered.has_more_than_a_page(tether).await? {
                    if let Err(e) = ordered.update(tether).await {
                        tracing::error!(
                            "Could not update scroller end cursor, it has been removed: `{e}`"
                        );
                        *self = MailScrollerState::NotSynced(ordered.clone());
                    }
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

    /// Check if there is more data available in order.
    pub async fn has_more_in_order(&self, tether: &Tether) -> Result<bool, StashError> {
        match self {
            MailScrollerState::Online(ordered) => ordered.has_more(tether).await,
            MailScrollerState::Offline { ordered, .. } => ordered.has_more(tether).await,
            _ => Ok(false),
        }
    }
}
