use proton_core_common::datatypes::LocalLabelId;
use stash::stash::{StashError, Tether};

use crate::{
    datatypes::ReadFilter,
    models::{CachedScrollData, ScrollData},
};

#[derive(Debug)]
pub enum MailScrollerState<T: ScrollData> {
    Online(CachedScrollData<T>),
    Offline {
        ordered: CachedScrollData<T>,
        unordered: CachedScrollData<T>,
    },
    NotSynced(CachedScrollData<T>),
    None,
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
            None => Ok(MailScrollerState::None),
        }
    }

    pub async fn new_not_synced(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        page_size: usize,
    ) -> Result<Self, StashError> {
        let unordered = CachedScrollData::all(local_label_id, unread, page_size).await?;

        Ok(MailScrollerState::NotSynced(unordered))
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

    pub fn offline_mut(&mut self) -> Option<&mut CachedScrollData<T>> {
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

    pub fn is_none(&self) -> bool {
        matches!(self, MailScrollerState::None)
    }

    pub async fn sync(
        &mut self,
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        page_size: usize,
        tether: &Tether,
    ) -> Result<(), StashError> {
        match self {
            MailScrollerState::Online(ref mut ordered)
            | MailScrollerState::Offline {
                ref mut ordered, ..
            } => {
                if !ordered.has_more_than_a_page(tether).await? {
                    ordered.update(tether).await?;
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
