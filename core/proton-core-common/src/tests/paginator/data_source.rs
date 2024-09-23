use crate::paginator::{DataSource, Paginator};
use serde::{Deserialize, Serialize};
use stash::macros::Model;
use stash::orm::{Model, ResultsetChange};
use stash::params;
use stash::stash::{AgnosticInterface, Interface, Stash, StashError};
use std::future::Future;
use std::num::NonZeroU32;
use std::ops::Range;
use tempdir::TempDir;
use tokio::sync::Mutex;

#[derive(Debug, Model, Eq, PartialEq, Clone, Serialize, Deserialize)]
#[TableName("test")]
pub struct TestModel {
    #[IdField]
    id: u64,

    #[RowIdField]
    #[serde(skip)]
    pub row_id: Option<u64>,
    #[StashField]
    #[serde(skip)]
    pub stash: Option<Stash>,
}

impl TestModel {
    /// Override save for create or ignore.
    pub async fn save(&mut self) -> Result<(), StashError> {
        let Some(stash) = self.stash.clone() else {
            return Err(StashError::NoStashAvailable);
        };

        self.save_using(&stash).await
    }

    /// Override save_using for create or ignore
    pub async fn save_using<A>(&mut self, interface: &A) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if let Some(element) = Self::find_first("WHERE id = ?", params![self.id], interface).await?
        {
            self.row_id = element.row_id;
            self.set_stash(element.stash().unwrap());
        } else {
            <Self as Model>::save_using(self, interface).await?;
        }

        Ok(())
    }
}
struct TestDataSource {
    total: usize,
}

impl TestDataSource {
    fn new() -> Self {
        Self { total: 18 }
    }
    async fn create_table(stash: &Stash) -> Result<(), StashError> {
        stash
            .execute("CREATE TABLE test (id INTEGER PRIMARY KEY)", vec![])
            .await?;
        Ok(())
    }
    async fn insert_pages(
        &self,
        range: Range<u32>,
        stash: &Stash,
    ) -> Result<Vec<TestModel>, StashError> {
        let tx = stash.transaction().await?;
        let mut result = Vec::with_capacity(range.len());
        for i in range.into_iter() {
            let mut value = TestModel {
                id: i.into(),
                row_id: None,
                stash: None,
            };
            value.save_using(&tx).await?;
            result.push(value);
        }
        tx.commit().await?;
        Ok(result)
    }
}
impl DataSource for TestDataSource {
    type Item = TestModel;
    type Error = StashError;

    fn total(&self, _: &Stash) -> impl Future<Output = Result<usize, Self::Error>> + Send {
        std::future::ready(Ok(self.total))
    }

    fn sync_first_page(
        &self,
        page_size: NonZeroU32,
        stash: &Stash,
    ) -> impl Future<Output = Result<Vec<Self::Item>, Self::Error>> + Send {
        async move { self.insert_pages(0u32..page_size.get(), stash).await }
    }

    fn sync_page_after(
        &self,
        cursor_index: u32,
        page_size: NonZeroU32,
        mut elements: Vec<Self::Item>,
        stash: &Stash,
    ) -> impl Future<Output = Result<Vec<Self::Item>, Self::Error>> + Send {
        let last_element = elements.pop().unwrap();
        if cursor_index <= 15 {
            assert_eq!(last_element.id, cursor_index as u64 - 1);
        } else {
            // last page.
            assert_eq!(last_element.id, 17);
        }
        let start = cursor_index.min(self.total.try_into().unwrap());
        let end = (start + page_size.get()).min(self.total.try_into().unwrap());
        async move {
            if start < self.total.try_into().unwrap() {
                self.insert_pages(start..end, stash).await
            } else {
                Ok(vec![])
            }
        }
    }
}

struct SkipFirstSyncSource(TestDataSource);

impl DataSource for SkipFirstSyncSource {
    type Item = TestModel;
    type Error = StashError;

    fn total(&self, stash: &Stash) -> impl Future<Output = Result<usize, Self::Error>> + Send {
        self.0.total(stash)
    }

    fn sync_first_page(
        &self,
        _: NonZeroU32,
        _: &Stash,
    ) -> impl Future<Output = Result<Vec<Self::Item>, Self::Error>> + Send {
        async {
            panic!("Should not be called");
        }
    }

    fn sync_page_after(
        &self,
        cursor_index: u32,
        page_size: NonZeroU32,
        elements: Vec<Self::Item>,
        stash: &Stash,
    ) -> impl Future<Output = Result<Vec<Self::Item>, Self::Error>> + Send {
        self.0
            .sync_page_after(cursor_index, page_size, elements, stash)
    }
}

struct IrregularPageDataSource {
    source: TestDataSource,
    pages: Mutex<Vec<Range<u32>>>,
}

impl IrregularPageDataSource {
    fn new(ranges: impl IntoIterator<Item = Range<u32>>) -> Self {
        let mut total = 0_usize;
        let mut ranges: Vec<Range<u32>> = ranges
            .into_iter()
            .inspect(|v| {
                total += v.len();
            })
            .collect();
        ranges.reverse();
        Self {
            source: TestDataSource { total },
            pages: Mutex::new(ranges),
        }
    }
}

impl DataSource for IrregularPageDataSource {
    type Item = TestModel;
    type Error = StashError;

    fn total(&self, _: &Stash) -> impl Future<Output = Result<usize, Self::Error>> + Send {
        std::future::ready(Ok(self.source.total))
    }

    fn sync_first_page(
        &self,
        _: NonZeroU32,
        stash: &Stash,
    ) -> impl Future<Output = Result<Vec<Self::Item>, Self::Error>> + Send {
        async move {
            if let Some(range) = self.pages.lock().await.pop() {
                return self.source.insert_pages(range, stash).await;
            }
            Ok(vec![])
        }
    }

    fn sync_page_after(
        &self,
        _: u32,
        _: NonZeroU32,
        elements: Vec<Self::Item>,
        stash: &Stash,
    ) -> impl Future<Output = Result<Vec<Self::Item>, Self::Error>> + Send {
        async move {
            if let Some(range) = self.pages.lock().await.pop() {
                assert_eq!(elements.last().unwrap().id, range.start as u64 - 1);
                return self.source.insert_pages(range, stash).await;
            }
            Ok(vec![])
        }
    }
}

#[tokio::test]
async fn data_source_sync() {
    let (stash, _dir) = init_db().await;

    let source = TestDataSource::new();

    let paginator = Paginator::new(
        "ORDER BY id ASC",
        vec![],
        &stash,
        NonZeroU32::new(5).unwrap(),
        Some(source),
        None,
    )
    .await
    .unwrap();

    // Check first page is downloaded
    check_range(&stash, 0u32..5u32).await;
    check_page(&stash, &paginator).await;

    // Check element [5..9] are available
    paginator.next_page().await.unwrap();
    check_range(&stash, 5u32..10u32).await;
    check_page(&stash, &paginator).await;

    // Check element [10..14] are available
    paginator.next_page().await.unwrap();
    check_range(&stash, 10u32..15u32).await;
    check_page(&stash, &paginator).await;

    // Check element [15..18] are available
    let values = paginator.next_page().await.unwrap();
    check_range_with_limit(&stash, 15u32..19u32, Some(3)).await;
    assert_eq!(values.len(), 3);

    // Check no new values are returned for the current page.
    assert!(!paginator.has_next_page().await);
    let last_values = paginator.next_page().await.unwrap();
    assert!(last_values.is_empty());
}

#[tokio::test]
async fn data_source_sync_first_page_if_existing_less_than_page_size() {
    // Check if the first sync is performed if some elements are present
    // but not more than page size.
    let (stash, _dir) = init_db().await;

    let source = TestDataSource::new();
    source.insert_pages(0..3_u32.into(), &stash).await.unwrap();

    let paginator = Paginator::new(
        "ORDER BY id ASC",
        vec![],
        &stash,
        NonZeroU32::new(5).unwrap(),
        Some(source),
        None,
    )
    .await
    .unwrap();

    // Check first page is downloaded
    check_range(&stash, 0u32..5u32).await;
    check_page(&stash, &paginator).await;
}

#[tokio::test]
async fn data_source_skips_sync_first_page_if_existing_greater_than_page_size() {
    // Check if the first sync is performed if some elements are present
    // but not more than page size.
    let (stash, _dir) = init_db().await;

    let source = TestDataSource::new();
    source.insert_pages(0..5_u32.into(), &stash).await.unwrap();

    let paginator = Paginator::new(
        "ORDER BY id ASC",
        vec![],
        &stash,
        NonZeroU32::new(5).unwrap(),
        Some(SkipFirstSyncSource(source)),
        None,
    )
    .await
    .unwrap();

    // Check first page is downloaded
    check_range(&stash, 0u32..5u32).await;
    check_page(&stash, &paginator).await;
}

#[tokio::test]
async fn data_source_sync_with_callback() {
    // The page number should not increase when new elements are added.
    let (stash, _dir) = init_db().await;
    let source = TestDataSource::new();
    let (sender, receiver) = flume::unbounded();

    let handle = tokio::spawn(async move {
        // We should receive exactly one notification.
        receiver.recv_async().await
    });

    let paginator = Paginator::new(
        "ORDER BY id ASC",
        vec![],
        &stash,
        NonZeroU32::new(5).unwrap(),
        Some(source),
        Some(sender),
    )
    .await
    .unwrap();

    assert_eq!(paginator.page_count().await, 4);
    check_page(&stash, &paginator).await;

    paginator.next_page().await.unwrap();
    assert_eq!(paginator.page_count().await, 4);
    check_page(&stash, &paginator).await;

    paginator.next_page().await.unwrap();
    assert_eq!(paginator.page_count().await, 4);
    check_page(&stash, &paginator).await;

    paginator.next_page().await.unwrap();
    assert_eq!(paginator.page_count().await, 4);
    check_page_with_limit(&stash, &paginator, Some(3)).await;

    // Check no new values are returned for the current page.
    assert!(!paginator.has_next_page().await);
    let last_values = paginator.next_page().await.unwrap();
    assert!(last_values.is_empty());

    // Insert new value
    let mut new_value = TestModel {
        id: 19,
        row_id: None,
        stash: Some(stash.clone()),
    };
    new_value.save_using(&stash).await.unwrap();

    drop(paginator);
    drop(stash);

    // We should only receive a notification for the manually inserted element.
    let notification = handle.await.unwrap().unwrap();
    assert_eq!(notification, ResultsetChange::Inserted(new_value));
}

#[tokio::test]
async fn data_source_irregular_pages() {
    // Check syncing logic for pages that do not have page number of elements.
    let (stash, _dir) = init_db().await;

    // Ranges represent the following sequence of (element range, expected page number)
    let pages = [
        // * Sync first page: 3 elements
        (0u32..3, 1u32),
        // * Sync next page: fetch remaining 2 elements, still first page
        (3..5, 1),
        // * Sync next page: Syncs full page, page 2
        (5..10, 2),
        // * Sync next page: Syncs 2 element, page 3
        (10..12, 3),
        // * Sync next page: Syncs 1 element, still page 3
        (12..13, 3),
        // * Sync next page: Syncs 2 elements, still page 3,
        (13..15, 3),
        // * Sync next page: Syncs full page, page 4
        (15..20, 4),
    ];

    let source = IrregularPageDataSource::new(pages.iter().map(|(range, _)| range.clone()));

    let paginator = Paginator::new(
        "ORDER BY id ASC",
        vec![],
        &stash,
        NonZeroU32::new(5).unwrap(),
        Some(source),
        None,
    )
    .await
    .unwrap();

    // Check initial sync
    assert_eq!(paginator.page_count().await as usize, 4);
    assert_eq!(paginator.current_page_number().await, 1);

    check_page_with_limit(&stash, &paginator, Some(pages[0].0.len())).await;

    for (range, page_num) in pages.into_iter().skip(1) {
        assert!(paginator.has_next_page().await);
        let next_elements = paginator.next_page().await.unwrap();
        assert_eq!(
            next_elements.len(),
            range.len(),
            "Failed to sync expected number of elements in range:{range:?}"
        );
        assert_eq!(
            paginator.current_page_number().await,
            page_num,
            "Page number does not match for range={range:?}"
        );
        for (index, id) in range.clone().into_iter().enumerate() {
            assert_eq!(
                next_elements[index].id,
                id as u64,
                "Element {index} does not match expected in range={range:?} elements={:?} ",
                next_elements.iter().map(|e| e.id).collect::<Vec<_>>()
            );
        }
    }

    assert!(!paginator.has_next_page().await);
}

async fn init_db() -> (Stash, TempDir) {
    let dir = TempDir::new("remote_sync").unwrap();
    let stash = Stash::new(Some(&dir.path().join("sqlite.db"))).unwrap();
    TestDataSource::create_table(&stash).await.unwrap();
    (stash, dir)
}

// Check the range of values is present in the database.
async fn check_range(stash: &Stash, range: Range<u32>) {
    check_range_with_limit(stash, range, None).await
}
async fn check_range_with_limit(stash: &Stash, range: Range<u32>, max_len: Option<usize>) {
    let start = range.start;
    let end = range.end;
    let iter = range.into_iter().map(|id| TestModel {
        id: id as u64,
        row_id: Some(id as u64),
        stash: Some(stash.clone()),
    });

    let expected = if let Some(max) = max_len {
        iter.take(max).collect::<Vec<_>>()
    } else {
        iter.collect::<Vec<_>>()
    };

    let values = TestModel::find(
        "WHERE id >= ? AND id < ? ORDER BY id ASC ",
        params![start, end],
        stash,
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        expected.len(),
        values.len(),
        "Range [{start}..{end}]: Expected and values have different lengths"
    );
    for (index, (expected, value)) in std::iter::zip(expected, values).into_iter().enumerate() {
        assert_eq!(
            expected, value,
            "Range [{start}..{end}]: Value at index {index}, does not match"
        );
    }
}

// Check the range of values is present in the current page.
async fn check_page<R: DataSource<Item = TestModel>>(
    stash: &Stash,
    paginator: &Paginator<TestModel, R>,
) {
    check_page_with_limit(stash, paginator, None).await
}

async fn check_page_with_limit<R: DataSource<Item = TestModel>>(
    stash: &Stash,
    paginator: &Paginator<TestModel, R>,
    max_len: Option<usize>,
) {
    let start =
        (paginator.current_page_number().await.saturating_sub(1)) * paginator.page_size().get();
    let end = (paginator.current_page_number().await) * paginator.page_size().get();
    let iter = (start..end).into_iter().map(|id| TestModel {
        id: id as u64,
        row_id: Some(id as u64),
        stash: Some(stash.clone()),
    });

    let expected = if let Some(max) = max_len {
        iter.take(max).collect::<Vec<_>>()
    } else {
        iter.collect::<Vec<_>>()
    };

    let values = paginator.current_page().await.unwrap();
    assert_eq!(
        expected.len(),
        values.len(),
        "Range [{start}..{end}]: Expected and values have different lengths"
    );
    for (index, (expected, value)) in std::iter::zip(expected, values).into_iter().enumerate() {
        assert_eq!(
            expected, value,
            "Range [{start}..{end}]: Value at index {index}, does not match"
        );
    }
}
