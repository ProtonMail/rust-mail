use crate::{
    MailContextError,
    mail_scroller::{MailScroller, MailScrollerItem},
};
use derive_more::Debug;
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::sync::oneshot;
use tracing::{debug, info, instrument, warn};
use uuid::Uuid;

/// Cursor over [`MailScroller`], used for the left/right swiping feature on
/// mobiles.
pub struct MailCursor<T>
where
    T: MailScrollerItem,
{
    id: Uuid,
    parent: Arc<MailScroller<T>>,
    siblings: RwLock<Siblings<T>>,
}

impl<T> MailCursor<T>
where
    T: MailScrollerItem,
{
    #[instrument(skip_all)]
    pub(crate) fn new(parent: Arc<MailScroller<T>>, looking_at: T::Id) -> Self {
        let id = Uuid::new_v4();
        let siblings = Siblings::of(&parent.items().read(), looking_at);

        info!(?id, ?looking_at, ?siblings, "Creating MailCursor");

        if let PrevSibling::None = siblings.prev
            && let NextSibling::None = siblings.next
        {
            warn!("Created a cursor for an item that's not present on the list");
        }

        Self {
            id,
            parent,
            siblings: RwLock::new(siblings),
        }
    }

    /// Returns the item that's right before the cursor.
    ///
    /// This function does retreat the cursor, see [`Self::goto_prev()`].
    #[instrument(skip_all)]
    pub fn peek_prev(&self) -> Option<T> {
        let mut siblings = self.siblings.upgradable_read();

        match siblings.prev {
            PrevSibling::None => None,

            PrevSibling::Some(prev) => {
                let items = self.parent.items().read();

                if let Some(prev) = Self::find(&items, prev) {
                    Some(prev)
                } else {
                    siblings.with_upgraded(|siblings| {
                        siblings.recover_prev(prev, |id| Self::find(&items, id))
                    })
                }
            }
        }
    }

    /// Returns the item that's right after the cursor.
    ///
    /// This function does not advance the cursor, see [`Self::goto_next()`].
    #[instrument(skip_all)]
    pub fn peek_next(&self) -> NextMailCursorItem<T> {
        let mut siblings = self.siblings.upgradable_read();

        match siblings.next {
            NextSibling::None => NextMailCursorItem::None,

            NextSibling::Some(next) => {
                let items = self.parent.items().read();

                if let Some(next) = Self::find(&items, next) {
                    NextMailCursorItem::Some(next)
                } else {
                    siblings.with_upgraded(|siblings| {
                        let next = siblings.recover_next(next, |id| Self::find(&items, id));

                        match next {
                            Some(next) => NextMailCursorItem::Some(next),
                            None => NextMailCursorItem::None,
                        }
                    })
                }
            }

            NextSibling::Maybe(_) => NextMailCursorItem::Maybe,
        }
    }

    /// Advances the cursor and returns the next item.
    ///
    /// This function should be called only if [`Self::peek_next()`] returned
    /// [`NextMailCursorItem::Maybe`], otherwise you should call
    /// [`Self::goto_next()`].
    #[instrument(skip_all, fields(id = ?self.id))]
    pub async fn fetch_next(&self) -> Result<Option<T>, MailContextError> {
        let (tx, rx) = oneshot::channel();

        self.parent.fetch_more(Some(tx))?;

        // Wait until the scroller is updated
        _ = rx.await;

        // ---

        let mut siblings = self.siblings.write();
        let items = self.parent.items().read();

        match siblings.next {
            NextSibling::None => Ok(None),
            NextSibling::Some(next) => Ok(Self::find(&items, next)),

            NextSibling::Maybe(next) => {
                *siblings = Siblings::of(&items, next);

                if let NextSibling::Some(next) = siblings.next {
                    Ok(Self::find(&items, next))
                } else {
                    // `Siblings::of()` assumes that if the next sibling is
                    // missing, we just have to fetch the next page to get it.
                    //
                    // In here we've just fetched the next page - if the sibling
                    // is still missing, it means we must've reached end of the
                    // folder / search, so we might as well note it down:
                    siblings.next = NextSibling::None;

                    Ok(None)
                }
            }
        }
    }

    /// Moves cursor one item backward.
    ///
    /// After this operation cursor's head will point at [`Self::peek_prev()`].
    #[instrument(skip_all, fields(id = ?self.id))]
    pub fn goto_prev(&self) {
        info!("Moving backwards");

        let mut siblings = self.siblings.write();
        let items = self.parent.items().read();

        if let PrevSibling::Some(prev) = siblings.prev {
            if Self::find(&items, prev).is_none() {
                siblings.recover_prev(prev, |id| Self::find(&items, id));
            } else {
                *siblings = Siblings::of(&items, prev);
            }

            debug!(?siblings, "Siblings updated");
        }
    }

    /// Moves cursor one item forward.
    ///
    /// After this operation cursor's head will point at [`Self::peek_next()`].
    #[instrument(skip_all, fields(id = ?self.id))]
    pub fn goto_next(&self) {
        info!("Moving forwards");

        let mut siblings = self.siblings.write();
        let items = self.parent.items().read();

        if let NextSibling::Some(next) = siblings.next {
            if Self::find(&items, next).is_none() {
                siblings.recover_next(next, |id| Self::find(&items, id));
            } else {
                *siblings = Siblings::of(&items, next);
            }

            debug!(?siblings, "Siblings updated");
        }
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

#[derive(Debug)]
struct Siblings<T>
where
    T: MailScrollerItem,
{
    prev: PrevSibling<T::Id>,
    next: NextSibling<T::Id>,

    /// Ids of all of the items we've seen - used for the recovery mechanism.
    ///
    /// See [`Self::recover_prev()`] and [`Self::recover_next()`].
    #[debug(skip)]
    witnesses: Vec<T::Id>,
}

impl<T> Siblings<T>
where
    T: MailScrollerItem,
{
    /// Returns siblings of `id`, i.e. the items right before and right after
    /// `id`.
    fn of(items: &[T], id: T::Id) -> Self {
        let witnesses: Vec<_> = items.iter().map(|item| item.item_id()).collect();

        let Some(idx) = witnesses.iter().position(|witness| *witness == id) else {
            return Self {
                prev: PrevSibling::None,
                next: NextSibling::None,
                witnesses,
            };
        };

        let idx_to_id = |idx| witnesses.get(idx).copied();

        let prev = match idx.checked_sub(1).and_then(idx_to_id) {
            Some(prev) => PrevSibling::Some(prev),
            None => PrevSibling::None,
        };

        let next = match idx.checked_add(1).and_then(idx_to_id) {
            Some(next) => NextSibling::Some(next),

            // Compared to `prev` above, when the `next` item is missing we
            // can't distinguish between:
            //
            // - there's nothing more because we're reached the end of the list,
            // - there's nothing more because we've reached the end of the page.
            //
            // Let's optimistically assume it's the latter, i.e. more elements
            // will arrive when user calls `fetch_next()`.
            None => NextSibling::Maybe(id),
        };

        Self {
            prev,
            next,
            witnesses,
        }
    }

    /// Adjusts `self.prev` so that it points at a new item if the previous one
    /// has disappeared.
    ///
    /// Let's say we're given:
    ///
    /// ```
    /// A B C D E F G  -- items
    ///       < . >    -- cursor (prev, curr, next)
    /// ```
    ///
    /// ... and let's say that `C` and `D` now disappear:
    ///
    /// ```
    /// A B E F G
    /// ```
    ///
    /// At this point `self.prev` points at a non-existing item `D` - the way we
    /// recover is by TODO
    fn recover_prev(&mut self, id: T::Id, f: impl Fn(T::Id) -> Option<T>) -> Option<T> {
        debug!("Lost prev-sibling, recovering");

        let idx = self.witnesses.iter().position(|id2| *id2 == id)?;

        for &id in self.witnesses.iter().take(idx).rev() {
            if let Some(item) = f(id) {
                debug!(?id, "Prev-sibling recovered");

                self.prev = PrevSibling::Some(id);

                return Some(item);
            }
        }

        self.prev = PrevSibling::None;

        None
    }

    fn recover_next(&mut self, id: T::Id, f: impl Fn(T::Id) -> Option<T>) -> Option<T> {
        debug!("Lost next-sibling, recovering");

        for &id in self.witnesses.iter().skip_while(|id2| **id2 != id) {
            if let Some(item) = f(id) {
                debug!(?id, "Next-sibling recovered");

                self.next = NextSibling::Some(id);

                return Some(item);
            }
        }

        self.next = NextSibling::None;

        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrevSibling<Id> {
    None,
    Some(Id),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NextSibling<Id> {
    None,
    Some(Id),
    Maybe(Id),
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

        fn change_include(&mut self, _: IncludeSwitch) {
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

        pub fn large() -> Vec<FakeItem> {
            vec![
                FakeItem::new(1, "the fate of ophelia"),
                FakeItem::new(2, "elizabeth taylor"),
                FakeItem::new(3, "opalite"),
                FakeItem::new(4, "father figure"),
                FakeItem::new(5, "eldest daughter"),
                FakeItem::new(6, "ruin the friendship"),
                FakeItem::new(7, "actually romantic"),
                FakeItem::new(8, "wi$h li$t"),
                FakeItem::new(9, "wood"),
                FakeItem::new(10, "cancelled"),
            ]
        }

        pub fn large_without(ids: &[u64]) -> Vec<FakeItem> {
            large()
                .into_iter()
                .filter(|item| !ids.contains(&item.id))
                .collect()
        }
    }

    #[tokio::test]
    async fn constructor() {
        let (_ctx, _source, scroller, _handle) = target(datasets::small()).await;

        // ---
        // Create a cursor for the first item on the list

        let cursor = scroller.clone().cursor(1);

        assert_eq!(None, cursor.peek_prev());

        assert_eq!(
            Some(FakeItem::new(2, "elizabeth taylor")),
            cursor.peek_next()
        );

        // ---
        // Create a cursor for the middle item on the list

        let cursor = scroller.clone().cursor(3);

        assert_eq!(
            Some(FakeItem::new(2, "elizabeth taylor")),
            cursor.peek_prev()
        );

        assert_eq!(Some(FakeItem::new(4, "father figure")), cursor.peek_next());

        // ---
        // Create a cursor for the last item on the list

        let cursor = scroller.clone().cursor(5);

        assert_eq!(Some(FakeItem::new(4, "father figure")), cursor.peek_prev());
        assert_eq!(NextMailCursorItem::Maybe, cursor.peek_next());

        // ---
        // Create a cursor for a non-existing item (legal, but suspicious)

        let cursor = scroller.clone().cursor(69420);

        assert_eq!(None, cursor.peek_prev());
        assert_eq!(None, cursor.peek_next());
    }

    #[tokio::test]
    async fn basic_backward_movement() {
        let (_ctx, _source, scroller, _handle) = target(datasets::small()).await;
        let cursor = scroller.clone().cursor(5);

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
        let cursor = scroller.clone().cursor(1);

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
        assert_eq!(None, cursor.peek_next());
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
        let cursor = scroller.clone().cursor(3);

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
        let cursor = scroller.clone().cursor(3);

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
        let cursor = scroller.clone().cursor(3);

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

    #[test]
    fn recover_prev() {
        let items = datasets::large();
        let mut target = Siblings::of(&items, 5);

        assert_eq!(PrevSibling::Some(4), target.prev);
        assert_eq!(NextSibling::Some(6), target.next);

        // ---
        // `prev` points at id=4 - now we're going to remove that item from the
        // list and the expectation is that `recover_prev()` replaces that id=4
        // with the closest item "to the left", which in this case is id=1.
        //
        // (considering that id=2 and id=3 get removed from the list as well.)

        let items = datasets::large_without(&[2, 3, 4]);

        let new_prev =
            target.recover_prev(4, |id| items.iter().find(|item| item.id == id).cloned());

        assert_eq!(Some(FakeItem::new(1, "the fate of ophelia")), new_prev);
        assert_eq!(PrevSibling::Some(1), target.prev);
        assert_eq!(NextSibling::Some(6), target.next);

        // ---

        let items = datasets::large_without(&[1, 2, 3, 4]);

        let new_prev =
            target.recover_prev(4, |id| items.iter().find(|item| item.id == id).cloned());

        assert_eq!(None, new_prev);
        assert_eq!(PrevSibling::None, target.prev);
        assert_eq!(NextSibling::Some(6), target.next);
    }

    #[test]
    fn recover_next() {
        let items = datasets::large();
        let mut target = Siblings::of(&items, 5);

        assert_eq!(PrevSibling::Some(4), target.prev);
        assert_eq!(NextSibling::Some(6), target.next);

        // ---

        let items = datasets::large_without(&[6, 7, 8]);

        let new_next =
            target.recover_next(6, |id| items.iter().find(|item| item.id == id).cloned());

        assert_eq!(Some(FakeItem::new(9, "wood")), new_next);
        assert_eq!(PrevSibling::Some(4), target.prev);
        assert_eq!(NextSibling::Some(9), target.next);

        // ---

        let items = datasets::large_without(&[6, 7, 8, 9, 10]);

        let new_next =
            target.recover_next(6, |id| items.iter().find(|item| item.id == id).cloned());

        assert_eq!(None, new_next);
        assert_eq!(PrevSibling::Some(4), target.prev);
        assert_eq!(NextSibling::None, target.next);
    }
}
