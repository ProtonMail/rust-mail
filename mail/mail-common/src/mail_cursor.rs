use crate::{
    MailContextError,
    mail_scroller::{MailScroller, MailScrollerItem},
};
use anyhow::anyhow;
use derive_more::Debug;
use itertools::Itertools;
use parking_lot::RwLock;
use std::sync::{Arc, Weak};
use tokio::sync::oneshot;
use tracing::{info, instrument, warn};
use uuid::Uuid;

/// Cursor over [`MailScroller`], used for the left/right swiping feature.
pub struct MailCursor<T>
where
    T: MailScrollerItem,
{
    id: Uuid,
    items: Weak<RwLock<Vec<T>>>,
    parent: Weak<MailScroller<T>>,
    state: RwLock<Option<State<T>>>,
}

impl<T> MailCursor<T>
where
    T: MailScrollerItem,
{
    #[instrument(skip_all)]
    pub(crate) fn new(
        looking_at: T::Id,
        items: Arc<RwLock<Vec<T>>>,
        parent: Arc<MailScroller<T>>,
    ) -> Self {
        let id = Uuid::new_v4();

        info!(?id, ?looking_at, "Creating MailCursor");

        let mut prevs: Vec<_> = items
            .read()
            .iter()
            .map(|item| item.item_id())
            .take_while_inclusive(|id| *id != looking_at)
            .collect();

        let state = if prevs.contains(&looking_at) {
            prevs.pop();

            Some(State {
                prevs,
                curr: looking_at,
                next: None,
            })
        } else {
            None
        };

        Self {
            id,
            parent: Arc::downgrade(&parent),
            items: Arc::downgrade(&items),
            state: RwLock::new(state),
        }
    }

    /// Returns the item that's before the cursor.
    ///
    /// This function does not retreat the cursor, see [`Self::goto_prev()`].
    pub fn peek_prev(&self) -> Option<T> {
        let mut state = self.state.write();
        let state = state.as_mut()?;
        let arc_items = self.items.upgrade()?;
        let items = arc_items.read();

        while let Some(prev) = state.prevs.last() {
            if let Some(prev) = Self::find(&items, *prev) {
                return Some(prev.clone());
            }

            state.prevs.pop();
        }

        None
    }

    /// Returns the item that's after the cursor.
    ///
    /// This function does not advance the cursor, see [`Self::goto_next()`].
    pub fn peek_next(&self) -> NextMailCursorItem<T> {
        let mut state = self.state.write();
        let Some(state) = state.as_mut() else {
            return NextMailCursorItem::None;
        };
        let Some(items) = self.items.upgrade() else {
            return NextMailCursorItem::None;
        };
        let items = items.read();

        if let Some(next) = state.next {
            if let Some(next) = Self::find(&items, next) {
                return NextMailCursorItem::Some(next);
            }

            state.next = None;
        }

        for item in items.iter() {
            if state.prevs.contains(&item.item_id()) {
                continue;
            }

            if state.curr == item.item_id() {
                continue;
            }

            state.next = Some(item.item_id());

            return NextMailCursorItem::Some(item.clone());
        }

        NextMailCursorItem::Maybe
    }

    /// Advances the cursor and returns the next item.
    ///
    /// This function should be called only if [`Self::peek_next()`] returned
    /// [`NextMailCursorItem::Maybe`], otherwise you should just call
    /// [`Self::goto_next()`].
    #[instrument(skip_all, fields(id = ?self.id))]
    pub async fn fetch_next(&self) -> Result<Option<T>, MailContextError> {
        let (tx, rx) = oneshot::channel();

        if let Some(parent) = self.parent.upgrade() {
            parent.fetch_more(Some(tx))?;
        } else {
            return Err(MailContextError::Other(anyhow!(
                "Parent scroller has been dropped"
            )));
        }

        // Wait until the scroller is updated
        _ = rx.await;

        // ---

        match self.peek_next() {
            NextMailCursorItem::Some(item) => {
                self.goto_next();

                Ok(Some(item))
            }

            _ => Ok(None),
        }
    }

    /// Moves cursor one item backward.
    ///
    /// After this operation cursor's head will point at [`Self::peek_prev()`].
    #[instrument(skip_all, fields(id = ?self.id))]
    pub fn goto_prev(&self) {
        info!("Moving backwards");

        let mut state = self.state.write();

        let Some(state) = state.as_mut() else {
            return;
        };

        if state.prevs.is_empty() {
            return;
        }

        state.next = Some(state.curr);
        state.curr = state.prevs.pop().unwrap();
    }

    /// Moves cursor one item forward.
    ///
    /// After this operation cursor's head will point at [`Self::peek_next()`].
    #[instrument(skip_all, fields(id = ?self.id))]
    pub fn goto_next(&self) {
        info!("Moving forwards");

        let mut state = self.state.write();

        let Some(state) = state.as_mut() else {
            return;
        };

        if state.next.is_none() {
            return;
        }

        state.prevs.push(state.curr);
        state.curr = state.next.take().unwrap();
    }

    fn find(items: &[T], id: T::Id) -> Option<T> {
        items.iter().find(|item| item.item_id() == id).cloned()
    }
}

impl<T> Drop for MailCursor<T>
where
    T: MailScrollerItem,
{
    #[instrument(skip_all, fields(id = ?self.id))]
    fn drop(&mut self) {
        info!("Dropping MailCursor");
    }
}

struct State<T>
where
    T: MailScrollerItem,
{
    prevs: Vec<T::Id>,
    curr: T::Id,
    next: Option<T::Id>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NextMailCursorItem<T> {
    None,
    Some(T),

    /// There might be some next item, but we're not sure due to pagination -
    /// call [`MailCursor::fetch_next()`] to advance the cursor.
    Maybe,
}

/// Comparing [`Option`] with [`MailCursorItem`] comes handy for tests,
/// otherwise the assertions look awkward.
#[cfg(test)]
impl<T> PartialEq<NextMailCursorItem<T>> for Option<T>
where
    T: PartialEq,
{
    fn eq(&self, other: &NextMailCursorItem<T>) -> bool {
        match (self, other) {
            (None, NextMailCursorItem::None) => true,
            (Some(lhs), NextMailCursorItem::Some(rhs)) => lhs == rhs,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        MailContextError, MailUserContext,
        datatypes::{IncludeSwitch, ReadFilter},
        mail_scroller::{MailPaginatorJoinHandle, MailScrollerHandle, MailScrollerSource},
        test_utils::test_context::MailTestContext,
    };
    use derive_more::Debug;
    use itertools::Either;
    use proton_core_api::services::proton::LabelId;
    use proton_core_common::datatypes::LocalLabelId;
    use proton_mail_common_derive::ScrollerEq;

    #[derive(Clone, Copy, Debug, PartialEq, ScrollerEq)]
    struct FakeItem {
        id: u64,
        title: &'static str,
    }

    impl FakeItem {
        fn new(id: u64, title: &'static str) -> Self {
            Self { id, title }
        }
    }

    impl MailScrollerItem for FakeItem {
        type Id = u64;

        fn item_id(&self) -> Self::Id {
            self.id
        }
    }

    #[derive(Clone, Debug)]
    struct FakeSource {
        items: Arc<RwLock<Vec<FakeItem>>>,
    }

    impl FakeSource {
        fn new(items: Vec<FakeItem>) -> Self {
            Self {
                items: Arc::new(RwLock::new(items)),
            }
        }

        fn set(&self, items: Vec<FakeItem>) {
            *self.items.write() = items;
        }
    }

    impl MailScrollerSource for FakeSource {
        type Item = FakeItem;

        async fn initialize(
            &mut self,
            _: &MailUserContext,
            _: flume::Sender<()>,
        ) -> Result<MailPaginatorJoinHandle, MailContextError> {
            Ok(None)
        }

        async fn visible_items(
            &self,
            _: &MailUserContext,
        ) -> Result<Vec<Self::Item>, MailContextError> {
            Ok(self.items.read().clone())
        }

        async fn seen_total(&self, _: &MailUserContext) -> Result<u64, MailContextError> {
            Ok(self.items.read().len() as u64)
        }

        async fn synced_total(&self, _: &MailUserContext) -> Result<u64, MailContextError> {
            Ok(self.items.read().len() as u64)
        }

        async fn all_total(&self, _: &MailUserContext) -> Result<u64, MailContextError> {
            Ok(self.items.read().len() as u64)
        }

        async fn has_more(&self, _: &MailUserContext) -> Result<bool, MailContextError> {
            Ok(false)
        }

        async fn sync_next(
            &mut self,
            _: &MailUserContext,
        ) -> Result<(Vec<Self::Item>, MailPaginatorJoinHandle), MailContextError> {
            todo!()
        }

        async fn sync_new(
            &mut self,
            _: &MailUserContext,
        ) -> Result<MailPaginatorJoinHandle, MailContextError> {
            todo!()
        }

        async fn change_filter(
            &mut self,
            _: &MailUserContext,
            _: ReadFilter,
        ) -> Result<MailPaginatorJoinHandle, MailContextError> {
            todo!()
        }

        fn change_include(&mut self, _: IncludeSwitch) -> Either<LocalLabelId, LabelId> {
            todo!()
        }

        async fn reset(
            &mut self,
            _: &MailUserContext,
        ) -> Result<MailPaginatorJoinHandle, MailContextError> {
            todo!()
        }

        fn watched_tables(&self) -> Vec<String> {
            Vec::new()
        }
    }

    async fn target(
        items: Vec<FakeItem>,
    ) -> (
        MailTestContext,
        FakeSource,
        Arc<MailScroller<FakeItem>>,
        MailScrollerHandle<FakeItem>,
    ) {
        let ctx = MailTestContext::new().await;
        let uctx = ctx.uninitialized_mail_user_context().await;
        let source = FakeSource::new(items);
        let page_size = 5;
        let supports_include_filter = false;

        let (scroller, handle) =
            MailScroller::new(uctx, source.clone(), page_size, supports_include_filter)
                .await
                .unwrap();

        scroller.force_refresh().unwrap();
        handle.updates.recv_async().await.unwrap();

        (ctx, source, Arc::new(scroller), handle)
    }

    mod datasets {
        use super::*;

        pub fn small() -> Vec<FakeItem> {
            vec![
                FakeItem::new(1, "the fate of ophelia"),
                FakeItem::new(2, "elizabeth taylor"),
                FakeItem::new(3, "opalite"),
                FakeItem::new(4, "father figure"),
                FakeItem::new(5, "eldest daughter"),
            ]
        }

        pub fn small_without(id: u64) -> Vec<FakeItem> {
            small().into_iter().filter(|item| item.id != id).collect()
        }
    }

    #[tokio::test]
    async fn constructor() {
        let (_ctx, _source, scroller, _handle) = target(datasets::small()).await;

        // ---
        // Create a cursor for the first item on the list

        let cursor = scroller.clone().cursor(1).await.unwrap();

        assert_eq!(None, cursor.peek_prev());

        assert_eq!(
            Some(FakeItem::new(2, "elizabeth taylor")),
            cursor.peek_next()
        );

        // ---
        // Create a cursor for the middle item on the list

        let cursor = scroller.clone().cursor(3).await.unwrap();

        assert_eq!(
            Some(FakeItem::new(2, "elizabeth taylor")),
            cursor.peek_prev()
        );

        assert_eq!(Some(FakeItem::new(4, "father figure")), cursor.peek_next());

        // ---
        // Create a cursor for the last item on the list

        let cursor = scroller.clone().cursor(5).await.unwrap();

        assert_eq!(Some(FakeItem::new(4, "father figure")), cursor.peek_prev());
        assert_eq!(NextMailCursorItem::Maybe, cursor.peek_next());

        // ---
        // Create a cursor for a non-existing item

        let cursor = scroller.clone().cursor(69420).await.unwrap();

        assert_eq!(None, cursor.peek_prev());
        assert_eq!(None, cursor.peek_next());
    }

    #[tokio::test]
    async fn basic_backward_movement() {
        let (_ctx, _source, scroller, _handle) = target(datasets::small()).await;
        let cursor = scroller.clone().cursor(5).await.unwrap();

        // ---
        // Sanity check of the initial state

        assert_eq!(Some(FakeItem::new(4, "father figure")), cursor.peek_prev());
        assert_eq!(NextMailCursorItem::Maybe, cursor.peek_next());

        // ---
        // Move from id=5 to id=4

        cursor.goto_prev();

        assert_eq!(Some(FakeItem::new(3, "opalite")), cursor.peek_prev());

        assert_eq!(
            Some(FakeItem::new(5, "eldest daughter")),
            cursor.peek_next()
        );

        // ---
        // Move from id=4 to id=3

        cursor.goto_prev();

        assert_eq!(
            Some(FakeItem::new(2, "elizabeth taylor")),
            cursor.peek_prev()
        );

        assert_eq!(Some(FakeItem::new(4, "father figure")), cursor.peek_next());

        // ---
        // Move from id=3 to id=2

        cursor.goto_prev();

        assert_eq!(
            Some(FakeItem::new(1, "the fate of ophelia")),
            cursor.peek_prev()
        );

        assert_eq!(Some(FakeItem::new(3, "opalite")), cursor.peek_next());

        // ---
        // Move from id=2 to id=1

        cursor.goto_prev();

        assert_eq!(None, cursor.peek_prev());

        assert_eq!(
            Some(FakeItem::new(2, "elizabeth taylor")),
            cursor.peek_next()
        );

        // ---
        // Move beyond the first item (does nothing)

        cursor.goto_prev();

        assert_eq!(None, cursor.peek_prev());

        assert_eq!(
            Some(FakeItem::new(2, "elizabeth taylor")),
            cursor.peek_next()
        );
    }

    #[tokio::test]
    async fn basic_forward_movement() {
        let (_ctx, _source, scroller, _handle) = target(datasets::small()).await;
        let cursor = scroller.clone().cursor(1).await.unwrap();

        // ---
        // Sanity check of the initial state

        assert_eq!(None, cursor.peek_prev());

        assert_eq!(
            Some(FakeItem::new(2, "elizabeth taylor")),
            cursor.peek_next()
        );

        // ---
        // Move from id=1 to id=2

        cursor.goto_next();

        assert_eq!(
            Some(FakeItem::new(1, "the fate of ophelia")),
            cursor.peek_prev()
        );

        assert_eq!(Some(FakeItem::new(3, "opalite")), cursor.peek_next());

        // ---
        // Move from id=2 to id=3

        cursor.goto_next();

        assert_eq!(
            Some(FakeItem::new(2, "elizabeth taylor")),
            cursor.peek_prev()
        );

        assert_eq!(Some(FakeItem::new(4, "father figure")), cursor.peek_next());

        // ---
        // Move from id=3 to id=4

        cursor.goto_next();

        assert_eq!(Some(FakeItem::new(3, "opalite")), cursor.peek_prev());

        assert_eq!(
            Some(FakeItem::new(5, "eldest daughter")),
            cursor.peek_next()
        );

        // ---
        // Move from id=4 to id=5

        cursor.goto_next();

        assert_eq!(Some(FakeItem::new(4, "father figure")), cursor.peek_prev());
        assert_eq!(NextMailCursorItem::Maybe, cursor.peek_next());

        // ---
        // Fetch next page, since `peek_next()` reported `Maybe`

        assert_eq!(None, cursor.fetch_next().await.unwrap());
    }

    /// Make sure the cursor behaves correctly when the item it's looking at
    /// disappears from the list.
    ///
    /// This is the case when you're browsing a list of messages / conversations
    /// with the `unread` filter active - as you continue swiping through the
    /// items, they become marked as read and disappear from the list.
    #[tokio::test]
    async fn nuke_current_item() {
        let (_ctx, source, scroller, handle) = target(datasets::small()).await;
        let cursor = scroller.clone().cursor(3).await.unwrap();

        // ---
        // Sanity check of the initial state

        assert_eq!(
            Some(FakeItem::new(2, "elizabeth taylor")),
            cursor.peek_prev()
        );

        assert_eq!(Some(FakeItem::new(4, "father figure")), cursor.peek_next());

        // ---
        // Nuke the item we're looking at

        source.set(datasets::small_without(3));
        scroller.force_refresh().unwrap();
        handle.updates.recv_async().await.unwrap();

        // ---
        // Make sure this didn't affect the cursor

        assert_eq!(
            Some(FakeItem::new(2, "elizabeth taylor")),
            cursor.peek_prev()
        );

        assert_eq!(Some(FakeItem::new(4, "father figure")), cursor.peek_next());

        // ---
        // Move from id=3 to id=4

        cursor.goto_next();

        assert_eq!(
            Some(FakeItem::new(2, "elizabeth taylor")),
            cursor.peek_prev(),
            "since we've nuked id=3, we see id=2 instead"
        );

        assert_eq!(
            Some(FakeItem::new(5, "eldest daughter")),
            cursor.peek_next()
        );

        // ---
        // Move from id=4 to id=2 (since id=3 doesn't exist anymore)

        cursor.goto_prev();

        assert_eq!(
            Some(FakeItem::new(1, "the fate of ophelia")),
            cursor.peek_prev()
        );

        assert_eq!(
            Some(FakeItem::new(4, "father figure")),
            cursor.peek_next(),
            "since we've nuked id=3, we see id=4 instead"
        );
    }

    /// Make sure the cursor behaves correctly when the prev-item disappears
    /// from the list.
    #[tokio::test]
    async fn nuke_previous_item() {
        let (_ctx, source, scroller, handle) = target(datasets::small()).await;
        let cursor = scroller.clone().cursor(3).await.unwrap();

        // ---
        // Sanity check of the initial state

        assert_eq!(
            Some(FakeItem::new(2, "elizabeth taylor")),
            cursor.peek_prev()
        );

        assert_eq!(Some(FakeItem::new(4, "father figure")), cursor.peek_next());

        // ---
        // Nuke the previous item

        source.set(datasets::small_without(2));
        scroller.force_refresh().unwrap();
        handle.updates.recv_async().await.unwrap();

        // ---
        // Make sure the cursor recovered

        assert_eq!(
            Some(FakeItem::new(1, "the fate of ophelia")),
            cursor.peek_prev(),
            "there's no id=2 anymore, so we 'fall backwards' to id=1"
        );

        assert_eq!(Some(FakeItem::new(4, "father figure")), cursor.peek_next());
    }

    /// Make sure the cursor behaves correctly when the next-item disappears
    /// from the list.
    #[tokio::test]
    async fn nuke_next_item() {
        let (_ctx, source, scroller, handle) = target(datasets::small()).await;
        let cursor = scroller.clone().cursor(3).await.unwrap();

        // ---
        // Sanity check of the initial state

        assert_eq!(
            Some(FakeItem::new(2, "elizabeth taylor")),
            cursor.peek_prev()
        );

        assert_eq!(Some(FakeItem::new(4, "father figure")), cursor.peek_next());

        // ---
        // Nuke the next item

        source.set(datasets::small_without(4));
        scroller.force_refresh().unwrap();
        handle.updates.recv_async().await.unwrap();

        // ---
        // Make sure the cursor recovered

        assert_eq!(
            Some(FakeItem::new(2, "elizabeth taylor")),
            cursor.peek_prev(),
        );

        assert_eq!(
            Some(FakeItem::new(5, "eldest daughter")),
            cursor.peek_next(),
            "there's no id=4 anymore, so we 'fall forwards' to id=5"
        );
    }
}
