use crate::paginator::{DataSource, Paginator};
use serde::{Deserialize, Serialize};
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, Stash, StashError, Tether};
use std::future::Future;
use std::num::NonZeroU32;
use std::ops::Range;
use tempdir::TempDir;

#[derive(Debug, Model, Eq, PartialEq, Clone, Serialize, Deserialize)]
#[TableName("test")]
pub struct TestModel {
    #[IdField]
    id: u64,
}

impl TestModel {
    /// Override `save` for create or ignore
    pub async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        if let Some(element) = Self::find_first("WHERE id = ?", params![self.id], bond).await? {
        } else {
            <Self as Model>::save(self, bond).await?;
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
    async fn create_table(tether: &mut Tether) -> Result<(), StashError> {
        tether
            .execute("CREATE TABLE test (id INTEGER PRIMARY KEY)", vec![])
            .await?;
        Ok(())
    }
    async fn insert_pages(
        &self,
        range: Range<u32>,
        tether: &mut Tether,
    ) -> Result<Vec<TestModel>, StashError> {
        let tx = tether.transaction().await?;
        let mut result = Vec::with_capacity(range.len());
        for i in range {
            let mut value = TestModel { id: i.into() };
            value.save(&tx).await?;
            result.push(value);
        }
        tx.commit().await?;
        Ok(result)
    }
}
impl DataSource for TestDataSource {
    type Item = TestModel;
    type Error = StashError;

    fn total(&self, _: &Tether) -> impl Future<Output = Result<usize, Self::Error>> + Send {
        std::future::ready(Ok(self.total))
    }

    async fn sync_first_page(
        &self,
        page_size: NonZeroU32,
        tether: &mut Tether,
    ) -> Result<Vec<Self::Item>, Self::Error> {
        self.insert_pages(0_u32..page_size.get(), tether).await
    }

    fn sync_page_after(
        &self,
        cursor_index: u32,
        page_size: NonZeroU32,
        mut elements: Vec<Self::Item>,
        tether: &mut Tether,
    ) -> impl Future<Output = Result<Vec<Self::Item>, Self::Error>> + Send {
        let last_element = elements.pop().unwrap();
        if cursor_index <= 15 {
            assert_eq!(last_element.id, u64::from(cursor_index) - 1);
        } else {
            // last page.
            assert_eq!(last_element.id, 17);
        }
        let start = cursor_index.min(self.total.try_into().unwrap());
        let end = (start + page_size.get()).min(self.total.try_into().unwrap());
        async move {
            if start < self.total.try_into().unwrap() {
                self.insert_pages(start..end, tether).await
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

    fn total(&self, tether: &Tether) -> impl Future<Output = Result<usize, Self::Error>> + Send {
        self.0.total(tether)
    }

    async fn sync_first_page(
        &self,
        _: NonZeroU32,
        _: &mut Tether,
    ) -> Result<Vec<Self::Item>, Self::Error> {
        panic!("Should not be called");
    }

    fn sync_page_after(
        &self,
        cursor_index: u32,
        page_size: NonZeroU32,
        elements: Vec<Self::Item>,
        tether: &mut Tether,
    ) -> impl Future<Output = Result<Vec<Self::Item>, Self::Error>> + Send {
        self.0
            .sync_page_after(cursor_index, page_size, elements, tether)
    }
}

#[tokio::test]
async fn data_source_sync() {
    let (stash, _dir) = init_db().await;
    let tether = stash.connection();

    let source = TestDataSource::new();

    let paginator = Paginator::new(
        "ORDER BY id ASC",
        vec![],
        &stash,
        NonZeroU32::new(5).unwrap(),
        source,
        true,
    )
    .await
    .unwrap();

    // Check first page is downloaded
    check_range(&tether, 0_u32..5_u32).await;
    check_page(&paginator).await;

    // Check element [5..9] are available
    paginator.next_page().await.unwrap();
    check_range(&tether, 5_u32..10_u32).await;
    check_page(&paginator).await;

    // Check element [10..14] are available
    paginator.next_page().await.unwrap();
    check_range(&tether, 10_u32..15_u32).await;
    check_page(&paginator).await;

    // Check element [15..18] are available
    let values = paginator.next_page().await.unwrap();
    check_range_with_limit(&tether, 15_u32..19_u32, Some(3)).await;
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
    let mut tether = stash.connection();
    let source = TestDataSource::new();

    source.insert_pages(0..3_u32, &mut tether).await.unwrap();

    let paginator = Paginator::new(
        "ORDER BY id ASC",
        vec![],
        &stash,
        NonZeroU32::new(5).unwrap(),
        source,
        true,
    )
    .await
    .unwrap();

    // Check first page is downloaded
    check_range(&tether, 0_u32..5_u32).await;
    check_page(&paginator).await;
}

#[tokio::test]
async fn data_source_skips_sync_first_page_if_existing_greater_than_page_size() {
    // Check if the first sync is performed if some elements are present
    // but not more than page size.
    let (stash, _dir) = init_db().await;
    let mut tether = stash.connection();

    let source = TestDataSource::new();
    source.insert_pages(0..5_u32, &mut tether).await.unwrap();

    let paginator = Paginator::new(
        "ORDER BY id ASC",
        vec![],
        &stash,
        NonZeroU32::new(5).unwrap(),
        SkipFirstSyncSource(source),
        true,
    )
    .await
    .unwrap();

    // Check first page is downloaded
    check_range(&tether, 0_u32..5_u32).await;
    check_page(&paginator).await;
}

#[tokio::test]
async fn data_source_sync_with_callback() {
    // The page number should not increase when new elements are added.
    let (stash, _dir) = init_db().await;
    let mut tether = stash.connection();
    let source = TestDataSource::new();

    let paginator = Paginator::new(
        "ORDER BY id ASC",
        vec![],
        &stash,
        NonZeroU32::new(5).unwrap(),
        source,
        true,
    )
    .await
    .unwrap();
    let handle = paginator.watch().unwrap();
    let receiver = &handle.receiver;

    assert_eq!(paginator.page_count().await, 4);
    check_page(&paginator).await;

    paginator.next_page().await.unwrap();
    assert_eq!(paginator.page_count().await, 4);
    check_page(&paginator).await;

    paginator.next_page().await.unwrap();
    assert_eq!(paginator.page_count().await, 4);
    check_page(&paginator).await;

    paginator.next_page().await.unwrap();
    assert_eq!(paginator.page_count().await, 4);
    check_page_with_limit(&paginator, Some(3)).await;

    // Check no new values are returned for the current page.
    assert!(!paginator.has_next_page().await);
    let last_values = paginator.next_page().await.unwrap();
    assert!(last_values.is_empty());

    // Insert new value
    let mut new_value = TestModel { id: 19 };
    let tx = tether.transaction().await.unwrap();
    new_value.save(&tx).await.unwrap();
    tx.commit().await.unwrap();

    drop(paginator);
    drop(tether);

    receiver.recv_async().await.unwrap();
}

async fn init_db() -> (Stash, TempDir) {
    let dir = TempDir::new("remote_sync").unwrap();
    let stash = Stash::new(Some(&dir.path().join("sqlite.db"))).unwrap();
    let mut tether = stash.connection();
    TestDataSource::create_table(&mut tether).await.unwrap();
    (stash, dir)
}

// Check the range of values is present in the database.
async fn check_range(tether: &Tether, range: Range<u32>) {
    check_range_with_limit(tether, range, None).await;
}
async fn check_range_with_limit(tether: &Tether, range: Range<u32>, max_len: Option<usize>) {
    let start = range.start;
    let end = range.end;
    let iter = range.into_iter().map(|id| TestModel { id: u64::from(id) });

    let expected = if let Some(max) = max_len {
        iter.take(max).collect::<Vec<_>>()
    } else {
        iter.collect::<Vec<_>>()
    };

    let values = TestModel::find(
        "WHERE id >= ? AND id < ? ORDER BY id ASC ",
        params![start, end],
        tether,
    )
    .await
    .unwrap();
    assert_eq!(
        expected.len(),
        values.len(),
        "Range [{start}..{end}]: Expected and values have different lengths"
    );
    for (index, (expected, value)) in std::iter::zip(expected, values).enumerate() {
        assert_eq!(
            expected, value,
            "Range [{start}..{end}]: Value at index {index}, does not match"
        );
    }
}

// Check the range of values is present in the current page.
async fn check_page<R: DataSource<Item = TestModel>>(paginator: &Paginator<TestModel, R>) {
    check_page_with_limit(paginator, None).await;
}

async fn check_page_with_limit<R: DataSource<Item = TestModel>>(
    paginator: &Paginator<TestModel, R>,
    max_len: Option<usize>,
) {
    let start =
        (paginator.current_page_number().await.saturating_sub(1)) * paginator.page_size().get();
    let end = (paginator.current_page_number().await) * paginator.page_size().get();
    let iter = (start..end).map(|id| TestModel { id: u64::from(id) });

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
    for (index, (expected, value)) in std::iter::zip(expected, values).enumerate() {
        assert_eq!(
            expected, value,
            "Range [{start}..{end}]: Value at index {index}, does not match"
        );
    }
}
