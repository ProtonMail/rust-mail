#![allow(non_snake_case)]

use crate::paginator::{DataSource, Paginator, Param};
use stash::macros::Model;
use stash::orm::Model;
use stash::orm::ResultsetChange;
use stash::stash::{Interface, Stash, StashError};
use std::future::Future;
use std::num::NonZeroUsize;

pub struct NullDataSource {}

impl DataSource for NullDataSource {
    type Item = TestModel;
    type Error = StashError;

    fn total(&self, _: &Stash) -> impl Future<Output = Result<usize, Self::Error>> + Send {
        std::future::ready(Ok(0))
    }

    fn sync_first_page(
        &self,
        _: NonZeroUsize,
        _: &Stash,
    ) -> impl Future<Output = Result<Vec<Self::Item>, Self::Error>> + Send {
        std::future::ready(Ok(vec![]))
    }

    fn sync_page_after(
        &self,
        _: usize,
        _: NonZeroUsize,
        _: Option<&Self::Item>,
        _: &Stash,
    ) -> impl Future<Output = Result<Vec<Self::Item>, Self::Error>> + Send {
        std::future::ready(Ok(vec![]))
    }
}

async fn create_table(stash: &Stash) {
    stash
        .execute(
            r"
			CREATE TABLE test_models (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                text TEXT NOT NULL,
                number INT NOT NULL
            )
        ",
            vec![],
        )
        .await
        .unwrap();
}

async fn create_records(stash: &Stash) {
    let tx = stash.transaction().await.unwrap();
    for i in 1..=1000 {
        let mut test = TestModel::new(format!("Test model #{i}"), i);
        test.save_using(&tx).await.unwrap();
    }
    tx.commit().await.unwrap();
}

pub async fn paginate_test_models(
    stash: &Stash,
) -> (
    Paginator<TestModel, NullDataSource>,
    flume::Receiver<ResultsetChange<TestModel, u64>>,
) {
    let (msg_sender, msg_receiver) = flume::unbounded();
    let paginator = Paginator::new(
        "WHERE number > ? ORDER BY number ASC",
        vec![Param::Integer(250)],
        stash,
        NonZeroUsize::new(50).unwrap(),
        None,
        Some(msg_sender),
    )
    .await
    .unwrap();
    (paginator, msg_receiver)
}

#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("test_models")]
pub struct TestModel {
    #[IdField(autoincrement)]
    pub id: Option<u64>,

    #[DbField]
    pub text: String,

    #[DbField]
    pub number: u32,

    #[RowIdField]
    pub row_id: Option<u64>,

    #[StashField]
    pub stash: Option<Stash>,
}

impl TestModel {
    fn new(text: String, number: u32) -> Self {
        Self {
            id: None,
            text,
            number,
            row_id: None,
            stash: None,
        }
    }
}

#[cfg(test)]
mod basic_pagination {
    use super::*;

    #[tokio::test]
    async fn baseline_setup() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");
        create_table(&stash).await;
        create_records(&stash).await;

        let (msg_sender, _msg_receiver) = flume::unbounded();
        let paginator: Paginator<TestModel, NullDataSource> = Paginator::new(
            "WHERE number > ? ORDER BY number ASC",
            vec![Param::Integer(250)],
            &stash,
            NonZeroUsize::new(50).unwrap(),
            None,
            Some(msg_sender),
        )
        .await
        .unwrap();

        assert_eq!(paginator.result_count().await, 50);
    }

    #[tokio::test]
    async fn current_page__start_of_set() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");
        create_table(&stash).await;
        create_records(&stash).await;

        let (msg_sender, _msg_receiver) = flume::unbounded();
        let paginator: Paginator<TestModel, NullDataSource> = Paginator::new(
            "ORDER BY number ASC",
            vec![],
            &stash,
            NonZeroUsize::new(5).unwrap(),
            None,
            Some(msg_sender),
        )
        .await
        .unwrap();

        let page = paginator.next_page().await.unwrap();
        assert_eq!(page.len(), 5);
        assert_eq!(
            page.into_iter()
                .map(|m| (m.text.clone(), m.number))
                .collect::<Vec<_>>(),
            vec![
                ("Test model #1".to_owned(), 1),
                ("Test model #2".to_owned(), 2),
                ("Test model #3".to_owned(), 3),
                ("Test model #4".to_owned(), 4),
                ("Test model #5".to_owned(), 5),
            ]
        );
    }

    #[tokio::test]
    async fn current_page__midway_through_set() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");
        create_table(&stash).await;
        create_records(&stash).await;

        let (msg_sender, _msg_receiver) = flume::unbounded();
        let paginator: Paginator<TestModel, NullDataSource> = Paginator::new(
            "WHERE number > ? ORDER BY number ASC",
            vec![Param::Integer(250)],
            &stash,
            NonZeroUsize::new(5).unwrap(),
            None,
            Some(msg_sender),
        )
        .await
        .unwrap();

        let page = paginator.next_page().await.unwrap();
        assert_eq!(page.len(), 5);
        assert_eq!(
            page.into_iter()
                .map(|m| (m.text.clone(), m.number))
                .collect::<Vec<_>>(),
            vec![
                ("Test model #251".to_owned(), 251),
                ("Test model #252".to_owned(), 252),
                ("Test model #253".to_owned(), 253),
                ("Test model #254".to_owned(), 254),
                ("Test model #255".to_owned(), 255),
            ]
        );
    }

    #[tokio::test]
    async fn next_page__midway_through_set() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");
        create_table(&stash).await;
        create_records(&stash).await;

        let (msg_sender, _msg_receiver) = flume::unbounded();
        let paginator: Paginator<TestModel, NullDataSource> = Paginator::new(
            "WHERE number > ? ORDER BY number ASC",
            vec![Param::Integer(250)],
            &stash,
            NonZeroUsize::new(5).unwrap(),
            None,
            Some(msg_sender),
        )
        .await
        .unwrap();

        let _ = paginator.next_page().await.unwrap();
        let page = paginator.next_page().await.unwrap();
        assert_eq!(page.len(), 5);
        assert_eq!(
            page.into_iter()
                .map(|m| (m.text.clone(), m.number))
                .collect::<Vec<_>>(),
            vec![
                ("Test model #256".to_owned(), 256),
                ("Test model #257".to_owned(), 257),
                ("Test model #258".to_owned(), 258),
                ("Test model #259".to_owned(), 259),
                ("Test model #260".to_owned(), 260),
            ]
        );
    }

    #[tokio::test]
    async fn previous_page__midway_through_set() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");
        create_table(&stash).await;
        create_records(&stash).await;

        let (msg_sender, _msg_receiver) = flume::unbounded();
        let paginator: Paginator<TestModel, NullDataSource> = Paginator::new(
            "WHERE number > ? ORDER BY number ASC",
            vec![Param::Integer(350)],
            &stash,
            NonZeroUsize::new(5).unwrap(),
            None,
            Some(msg_sender),
        )
        .await
        .unwrap();

        let page = paginator.next_page().await.unwrap();
        assert_eq!(page.len(), 5);
        assert_eq!(
            page.into_iter()
                .map(|m| (m.text.clone(), m.number))
                .collect::<Vec<_>>(),
            vec![
                ("Test model #351".to_owned(), 351),
                ("Test model #352".to_owned(), 352),
                ("Test model #353".to_owned(), 353),
                ("Test model #354".to_owned(), 354),
                ("Test model #355".to_owned(), 355),
            ]
        );
    }

    #[tokio::test]
    async fn reload_does_not_panic_on_first_page() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");
        create_table(&stash).await;

        let (msg_sender, _msg_receiver) = flume::unbounded();
        let paginator: Paginator<TestModel, NullDataSource> = Paginator::new(
            "WHERE number > ? ORDER BY number ASC",
            vec![Param::Integer(250)],
            &stash,
            NonZeroUsize::new(5).unwrap(),
            None,
            Some(msg_sender),
        )
        .await
        .unwrap();

        paginator.reload().await.unwrap();
    }
}

#[cfg(test)]
mod extended_pagination {
    use super::*;

    #[tokio::test]
    async fn navigate_several_pages() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");
        create_table(&stash).await;
        create_records(&stash).await;

        let (msg_sender, _msg_receiver) = flume::unbounded();
        let paginator: Paginator<TestModel, NullDataSource> = Paginator::new(
            "WHERE number > ? ORDER BY number ASC",
            vec![Param::Integer(500)],
            &stash,
            NonZeroUsize::new(10).unwrap(),
            None,
            Some(msg_sender),
        )
        .await
        .unwrap();

        let page1 = paginator.next_page().await.unwrap();
        assert_eq!(page1.len(), 10);
        assert_eq!(
            page1
                .into_iter()
                .map(|m| (m.text.clone(), m.number))
                .collect::<Vec<_>>(),
            vec![
                ("Test model #501".to_owned(), 501),
                ("Test model #502".to_owned(), 502),
                ("Test model #503".to_owned(), 503),
                ("Test model #504".to_owned(), 504),
                ("Test model #505".to_owned(), 505),
                ("Test model #506".to_owned(), 506),
                ("Test model #507".to_owned(), 507),
                ("Test model #508".to_owned(), 508),
                ("Test model #509".to_owned(), 509),
                ("Test model #510".to_owned(), 510),
            ]
        );

        let page2 = paginator.next_page().await.unwrap();
        assert_eq!(page2.len(), 10);
        assert_eq!(
            page2
                .into_iter()
                .map(|m| (m.text.clone(), m.number))
                .collect::<Vec<_>>(),
            vec![
                ("Test model #511".to_owned(), 511),
                ("Test model #512".to_owned(), 512),
                ("Test model #513".to_owned(), 513),
                ("Test model #514".to_owned(), 514),
                ("Test model #515".to_owned(), 515),
                ("Test model #516".to_owned(), 516),
                ("Test model #517".to_owned(), 517),
                ("Test model #518".to_owned(), 518),
                ("Test model #519".to_owned(), 519),
                ("Test model #520".to_owned(), 520),
            ]
        );

        let page3 = paginator.next_page().await.unwrap();
        assert_eq!(page3.len(), 10);
        assert_eq!(
            page3
                .into_iter()
                .map(|m| (m.text.clone(), m.number))
                .collect::<Vec<_>>(),
            vec![
                ("Test model #521".to_owned(), 521),
                ("Test model #522".to_owned(), 522),
                ("Test model #523".to_owned(), 523),
                ("Test model #524".to_owned(), 524),
                ("Test model #525".to_owned(), 525),
                ("Test model #526".to_owned(), 526),
                ("Test model #527".to_owned(), 527),
                ("Test model #528".to_owned(), 528),
                ("Test model #529".to_owned(), 529),
                ("Test model #530".to_owned(), 530),
            ]
        );
    }

    #[tokio::test]
    async fn reload_after_navigating_forwards() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");
        create_table(&stash).await;
        create_records(&stash).await;

        let (msg_sender, _msg_receiver) = flume::unbounded();
        let paginator: Paginator<TestModel, NullDataSource> = Paginator::new(
            "WHERE number > ? ORDER BY number ASC",
            vec![Param::Integer(600)],
            &stash,
            NonZeroUsize::new(5).unwrap(),
            None,
            Some(msg_sender),
        )
        .await
        .unwrap();

        let page1 = paginator.next_page().await.unwrap();
        assert_eq!(page1.len(), 5);

        let page2 = paginator.next_page().await.unwrap();
        assert_eq!(page2.len(), 5);

        let page3 = paginator.next_page().await.unwrap();
        assert_eq!(page3.len(), 5);

        let page4 = paginator.next_page().await.unwrap();
        assert_eq!(page4.len(), 5);

        let page5 = paginator.next_page().await.unwrap();
        assert_eq!(page5.len(), 5);

        let reloaded = paginator.reload().await.unwrap();
        assert_eq!(
            reloaded
                .into_iter()
                .map(|m| (m.text.clone(), m.number))
                .collect::<Vec<_>>(),
            vec![
                ("Test model #601".to_owned(), 601),
                ("Test model #602".to_owned(), 602),
                ("Test model #603".to_owned(), 603),
                ("Test model #604".to_owned(), 604),
                ("Test model #605".to_owned(), 605),
                ("Test model #606".to_owned(), 606),
                ("Test model #607".to_owned(), 607),
                ("Test model #608".to_owned(), 608),
                ("Test model #609".to_owned(), 609),
                ("Test model #610".to_owned(), 610),
                ("Test model #611".to_owned(), 611),
                ("Test model #612".to_owned(), 612),
                ("Test model #613".to_owned(), 613),
                ("Test model #614".to_owned(), 614),
                ("Test model #615".to_owned(), 615),
                ("Test model #616".to_owned(), 616),
                ("Test model #617".to_owned(), 617),
                ("Test model #618".to_owned(), 618),
                ("Test model #619".to_owned(), 619),
                ("Test model #620".to_owned(), 620),
                ("Test model #621".to_owned(), 621),
                ("Test model #622".to_owned(), 622),
                ("Test model #623".to_owned(), 623),
                ("Test model #624".to_owned(), 624),
                ("Test model #625".to_owned(), 625),
            ]
        );
    }
}
