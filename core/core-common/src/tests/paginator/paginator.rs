#![allow(non_snake_case)]

use crate::paginator::{DataSource, Paginator, Param};
use stash::macros::Model;
use stash::orm::Model;
use stash::stash::{Stash, StashError, Tether};
use std::future::Future;
use std::num::NonZeroU32;

struct NullDataSource {}

impl DataSource for NullDataSource {
    type Item = TestModel;
    type Error = StashError;

    fn total(&self, _: &Tether) -> impl Future<Output = Result<usize, Self::Error>> + Send {
        std::future::ready(Ok(0))
    }

    fn sync_first_page(
        &self,
        _: NonZeroU32,
        _: &mut Tether,
    ) -> impl Future<Output = Result<Vec<Self::Item>, Self::Error>> + Send {
        std::future::ready(Ok(vec![]))
    }

    fn sync_page_after(
        &self,
        _: u32,
        _: NonZeroU32,
        _: Vec<Self::Item>,
        _: &mut Tether,
    ) -> impl Future<Output = Result<Vec<Self::Item>, Self::Error>> + Send {
        std::future::ready(Ok(vec![]))
    }
}

async fn create_table(tether: &mut Tether) {
    tether
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

async fn create_records(tether: &mut Tether) {
    let tx = tether.transaction().await.unwrap();
    for i in 1..=1000 {
        let mut test = TestModel::new(format!("Test model #{i}"), i);
        test.save(&tx).await.unwrap();
    }
    tx.commit().await.unwrap();
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
}

impl TestModel {
    fn new(text: String, number: u32) -> Self {
        Self {
            id: None,
            text,
            number,
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
        let mut tether = stash.connection();
        create_table(&mut tether).await;
        create_records(&mut tether).await;

        let data_source = NullDataSource {};
        let paginator: Paginator<TestModel, NullDataSource> = Paginator::new(
            "WHERE number > ? ORDER BY number ASC",
            vec![Param::Integer(250)],
            &stash,
            NonZeroU32::new(50).unwrap(),
            data_source,
            true,
        )
        .await
        .unwrap();

        assert_eq!(paginator.result_count().await, 50);
        assert_eq!(paginator.current_page_number().await, 1);
    }

    #[tokio::test]
    async fn current_page__start_of_set() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");
        let mut tether = stash.connection();
        create_table(&mut tether).await;
        create_records(&mut tether).await;

        let data_source = NullDataSource {};
        let paginator: Paginator<TestModel, NullDataSource> = Paginator::new(
            "ORDER BY number ASC",
            vec![],
            &stash,
            NonZeroU32::new(5).unwrap(),
            data_source,
            true,
        )
        .await
        .unwrap();

        assert_eq!(paginator.current_page_number().await, 1);
        let page = paginator.current_page().await.unwrap();
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
        let mut tether = stash.connection();
        create_table(&mut tether).await;
        create_records(&mut tether).await;

        let data_source = NullDataSource {};
        let paginator: Paginator<TestModel, NullDataSource> = Paginator::new(
            "WHERE number > ? ORDER BY number ASC",
            vec![Param::Integer(250)],
            &stash,
            NonZeroU32::new(5).unwrap(),
            data_source,
            true,
        )
        .await
        .unwrap();

        assert_eq!(paginator.current_page_number().await, 1);
        let page = paginator.current_page().await.unwrap();
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
        let mut tether = stash.connection();
        create_table(&mut tether).await;
        create_records(&mut tether).await;

        let data_source = NullDataSource {};
        let paginator: Paginator<TestModel, NullDataSource> = Paginator::new(
            "WHERE number > ? ORDER BY number ASC",
            vec![Param::Integer(250)],
            &stash,
            NonZeroU32::new(5).unwrap(),
            data_source,
            true,
        )
        .await
        .unwrap();

        assert_eq!(paginator.current_page_number().await, 1);
        let page = paginator.next_page().await.unwrap();
        assert_eq!(paginator.current_page_number().await, 2);
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
        let mut tether = stash.connection();
        create_table(&mut tether).await;
        create_records(&mut tether).await;

        let data_source = NullDataSource {};
        let paginator: Paginator<TestModel, NullDataSource> = Paginator::new(
            "WHERE number > ? ORDER BY number ASC",
            vec![Param::Integer(350)],
            &stash,
            NonZeroU32::new(5).unwrap(),
            data_source,
            true,
        )
        .await
        .unwrap();

        assert_eq!(paginator.current_page_number().await, 1);
        _ = paginator.next_page().await.unwrap();
        assert_eq!(paginator.current_page_number().await, 2);
        let page = paginator.previous_page().await.unwrap();
        assert_eq!(paginator.current_page_number().await, 1);
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
        let mut tether = stash.connection();
        create_table(&mut tether).await;

        let data_source = NullDataSource {};
        let paginator: Paginator<TestModel, NullDataSource> = Paginator::new(
            "WHERE number > ? ORDER BY number ASC",
            vec![Param::Integer(250)],
            &stash,
            NonZeroU32::new(5).unwrap(),
            data_source,
            true,
        )
        .await
        .unwrap();

        paginator.reload().await.unwrap();
    }
}

#[cfg(test)]
mod extended_pagination {
    use super::*;

    #[allow(clippy::too_many_lines)]
    #[tokio::test]
    async fn navigate_several_pages() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");
        let mut tether = stash.connection();
        create_table(&mut tether).await;
        create_records(&mut tether).await;

        let data_source = NullDataSource {};
        let paginator: Paginator<TestModel, NullDataSource> = Paginator::new(
            "WHERE number > ? ORDER BY number ASC",
            vec![Param::Integer(500)],
            &stash,
            NonZeroU32::new(10).unwrap(),
            data_source,
            true,
        )
        .await
        .unwrap();

        assert_eq!(paginator.current_page_number().await, 1);
        let page1 = paginator.current_page().await.unwrap();
        assert_eq!(paginator.current_page_number().await, 1);
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
        assert_eq!(paginator.current_page_number().await, 2);
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
        assert_eq!(paginator.current_page_number().await, 3);
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

        let page2b = paginator.previous_page().await.unwrap();
        assert_eq!(paginator.current_page_number().await, 2);
        assert_eq!(page2b.len(), 10);
        assert_eq!(
            page2b
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
    }

    #[tokio::test]
    async fn reload_after_navigating_forwards() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");
        let mut tether = stash.connection();
        create_table(&mut tether).await;
        create_records(&mut tether).await;

        let data_source = NullDataSource {};
        let paginator: Paginator<TestModel, NullDataSource> = Paginator::new(
            "WHERE number > ? ORDER BY number ASC",
            vec![Param::Integer(600)],
            &stash,
            NonZeroU32::new(5).unwrap(),
            data_source,
            true,
        )
        .await
        .unwrap();

        assert_eq!(paginator.current_page_number().await, 1);
        let page1 = paginator.current_page().await.unwrap();
        assert_eq!(paginator.current_page_number().await, 1);
        assert_eq!(page1.len(), 5);

        let page2 = paginator.next_page().await.unwrap();
        assert_eq!(paginator.current_page_number().await, 2);
        assert_eq!(page2.len(), 5);

        let page3 = paginator.next_page().await.unwrap();
        assert_eq!(paginator.current_page_number().await, 3);
        assert_eq!(page3.len(), 5);

        let page4 = paginator.next_page().await.unwrap();
        assert_eq!(paginator.current_page_number().await, 4);
        assert_eq!(page4.len(), 5);

        let page5 = paginator.next_page().await.unwrap();
        assert_eq!(paginator.current_page_number().await, 5);
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

    #[tokio::test]
    async fn reload_after_navigating_forward_then_back() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");
        let mut tether = stash.connection();
        create_table(&mut tether).await;
        create_records(&mut tether).await;

        let data_source = NullDataSource {};
        let paginator: Paginator<TestModel, NullDataSource> = Paginator::new(
            "WHERE number > ? ORDER BY number ASC",
            vec![Param::Integer(600)],
            &stash,
            NonZeroU32::new(5).unwrap(),
            data_source,
            true,
        )
        .await
        .unwrap();

        assert_eq!(paginator.current_page_number().await, 1);
        let page1 = paginator.current_page().await.unwrap();
        assert_eq!(paginator.current_page_number().await, 1);
        assert_eq!(page1.len(), 5);

        let page2 = paginator.next_page().await.unwrap();
        assert_eq!(paginator.current_page_number().await, 2);
        assert_eq!(page2.len(), 5);

        let page3 = paginator.next_page().await.unwrap();
        assert_eq!(paginator.current_page_number().await, 3);
        assert_eq!(page3.len(), 5);

        let page4 = paginator.next_page().await.unwrap();
        assert_eq!(paginator.current_page_number().await, 4);
        assert_eq!(page4.len(), 5);

        let page5 = paginator.next_page().await.unwrap();
        assert_eq!(paginator.current_page_number().await, 5);
        assert_eq!(page5.len(), 5);

        let _page6 = paginator.next_page().await.unwrap();
        assert_eq!(paginator.current_page_number().await, 6);
        assert_eq!(page5.len(), 5);

        let page5b = paginator.previous_page().await.unwrap();
        assert_eq!(paginator.current_page_number().await, 5);
        assert_eq!(page5b.len(), 5);

        let page4b = paginator.previous_page().await.unwrap();
        assert_eq!(paginator.current_page_number().await, 4);
        assert_eq!(page4b.len(), 5);

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
            ]
        );
    }
}

#[cfg(test)]
mod changes_during_pagination {
    use super::*;
    use stash::params;

    #[allow(clippy::too_many_lines)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    async fn previous_page__changes_to_data_seen() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");
        let mut tether = stash.connection();
        create_table(&mut tether).await;
        create_records(&mut tether).await;

        let data_source = NullDataSource {};
        let paginator: Paginator<TestModel, NullDataSource> = Paginator::new(
            "WHERE number > ? ORDER BY number ASC",
            vec![Param::Integer(100)],
            &stash,
            NonZeroU32::new(5).unwrap(),
            data_source,
            true,
        )
        .await
        .unwrap();

        assert_eq!(paginator.current_page_number().await, 1);
        _ = paginator.next_page().await.unwrap();
        assert_eq!(paginator.current_page_number().await, 2);
        _ = paginator.next_page().await.unwrap();
        assert_eq!(paginator.current_page_number().await, 3);

        let tx = tether.transaction().await.unwrap();
        tx.execute(r"DELETE FROM test_models WHERE number = ?", params![102])
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let page2 = paginator.previous_page().await.unwrap();
        assert_eq!(paginator.current_page_number().await, 2);
        assert_eq!(page2.len(), 5);
        assert_eq!(
            page2
                .into_iter()
                .map(|m| (m.text.clone(), m.number))
                .collect::<Vec<_>>(),
            vec![
                ("Test model #107".to_owned(), 107),
                ("Test model #108".to_owned(), 108),
                ("Test model #109".to_owned(), 109),
                ("Test model #110".to_owned(), 110),
                ("Test model #111".to_owned(), 111),
            ]
        );

        let page1 = paginator.previous_page().await.unwrap();
        assert_eq!(paginator.current_page_number().await, 1);
        assert_eq!(page1.len(), 5);
        assert_eq!(
            page1
                .into_iter()
                .map(|m| (m.text.clone(), m.number))
                .collect::<Vec<_>>(),
            vec![
                ("Test model #101".to_owned(), 101),
                ("Test model #103".to_owned(), 103),
                ("Test model #104".to_owned(), 104),
                ("Test model #105".to_owned(), 105),
                ("Test model #106".to_owned(), 106),
            ]
        );

        _ = paginator.next_page().await.unwrap();
        assert_eq!(paginator.current_page_number().await, 2);
        let tx = tether.transaction().await.unwrap();
        // Add a new record in the middle of previous page
        let mut test = TestModel::new("Test model #102".to_string(), 102);
        test.save(&tx).await.unwrap();
        tx.commit().await.unwrap();

        let page3 = paginator.next_page().await.unwrap();
        assert_eq!(paginator.current_page_number().await, 3);
        assert_eq!(page3.len(), 5);
        assert_eq!(
            page3
                .into_iter()
                .map(|m| (m.text.clone(), m.number))
                .collect::<Vec<_>>(),
            vec![
                ("Test model #111".to_owned(), 111),
                ("Test model #112".to_owned(), 112),
                ("Test model #113".to_owned(), 113),
                ("Test model #114".to_owned(), 114),
                ("Test model #115".to_owned(), 115),
            ]
        );

        let all = paginator.reload().await.unwrap();
        assert_eq!(
            all.into_iter()
                .map(|m| (m.text.clone(), m.number))
                .collect::<Vec<_>>(),
            vec![
                ("Test model #101".to_owned(), 101),
                ("Test model #102".to_owned(), 102),
                ("Test model #103".to_owned(), 103),
                ("Test model #104".to_owned(), 104),
                ("Test model #105".to_owned(), 105),
                ("Test model #106".to_owned(), 106),
                ("Test model #107".to_owned(), 107),
                ("Test model #108".to_owned(), 108),
                ("Test model #109".to_owned(), 109),
                ("Test model #110".to_owned(), 110),
                ("Test model #111".to_owned(), 111),
                ("Test model #112".to_owned(), 112),
                ("Test model #113".to_owned(), 113),
                ("Test model #114".to_owned(), 114),
                ("Test model #115".to_owned(), 115),
            ]
        );

        let tx = tether.transaction().await.unwrap();
        tx.execute(
            r"DELETE FROM test_models WHERE number in (?,?,?,?)",
            params![103, 104, 105, 106],
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let all = paginator.reload().await.unwrap();
        assert_eq!(
            all.into_iter()
                .map(|m| (m.text.clone(), m.number))
                .collect::<Vec<_>>(),
            vec![
                ("Test model #101".to_owned(), 101),
                ("Test model #102".to_owned(), 102),
                ("Test model #107".to_owned(), 107),
                ("Test model #108".to_owned(), 108),
                ("Test model #109".to_owned(), 109),
                ("Test model #110".to_owned(), 110),
                ("Test model #111".to_owned(), 111),
                ("Test model #112".to_owned(), 112),
                ("Test model #113".to_owned(), 113),
                ("Test model #114".to_owned(), 114),
                ("Test model #115".to_owned(), 115),
                ("Test model #116".to_owned(), 116),
                ("Test model #117".to_owned(), 117),
                ("Test model #118".to_owned(), 118),
                ("Test model #119".to_owned(), 119),
            ]
        );

        let tx = tether.transaction().await.unwrap();
        for i in 103..=106 {
            let mut test = TestModel::new(format!("Test model #{i}"), i);
            test.save(&tx).await.unwrap();
        }
        tx.commit().await.unwrap();

        let all = paginator.reload().await.unwrap();
        assert_eq!(
            all.into_iter()
                .map(|m| (m.text.clone(), m.number))
                .collect::<Vec<_>>(),
            vec![
                ("Test model #101".to_owned(), 101),
                ("Test model #102".to_owned(), 102),
                ("Test model #103".to_owned(), 103),
                ("Test model #104".to_owned(), 104),
                ("Test model #105".to_owned(), 105),
                ("Test model #106".to_owned(), 106),
                ("Test model #107".to_owned(), 107),
                ("Test model #108".to_owned(), 108),
                ("Test model #109".to_owned(), 109),
                ("Test model #110".to_owned(), 110),
                ("Test model #111".to_owned(), 111),
                ("Test model #112".to_owned(), 112),
                ("Test model #113".to_owned(), 113),
                ("Test model #114".to_owned(), 114),
                ("Test model #115".to_owned(), 115),
                ("Test model #116".to_owned(), 116),
                ("Test model #117".to_owned(), 117),
                ("Test model #118".to_owned(), 118),
                ("Test model #119".to_owned(), 119),
            ]
        );

        let tx = tether.transaction().await.unwrap();
        tx.execute(
            r"DELETE FROM test_models WHERE number > ? AND number < ?",
            params![100, 120],
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let all = paginator.reload().await.unwrap();
        assert_eq!(
            all.into_iter()
                .map(|m| (m.text.clone(), m.number))
                .collect::<Vec<_>>(),
            vec![
                ("Test model #120".to_owned(), 120),
                ("Test model #121".to_owned(), 121),
                ("Test model #122".to_owned(), 122),
                ("Test model #123".to_owned(), 123),
                ("Test model #124".to_owned(), 124),
                ("Test model #125".to_owned(), 125),
                ("Test model #126".to_owned(), 126),
                ("Test model #127".to_owned(), 127),
                ("Test model #128".to_owned(), 128),
                ("Test model #129".to_owned(), 129),
                ("Test model #130".to_owned(), 130),
                ("Test model #131".to_owned(), 131),
                ("Test model #132".to_owned(), 132),
                ("Test model #133".to_owned(), 133),
                ("Test model #134".to_owned(), 134),
                ("Test model #135".to_owned(), 135),
                ("Test model #136".to_owned(), 136),
                ("Test model #137".to_owned(), 137),
                ("Test model #138".to_owned(), 138),
                ("Test model #139".to_owned(), 139),
            ]
        );
        let tx = tether.transaction().await.unwrap();
        for i in 100..=115 {
            let mut test = TestModel::new(format!("Test model #{i}"), i);
            test.save(&tx).await.unwrap();
        }
        tx.commit().await.unwrap();
        let all = paginator.reload().await.unwrap();
        assert_eq!(
            all.into_iter()
                .map(|m| (m.text.clone(), m.number))
                .collect::<Vec<_>>(),
            vec![
                ("Test model #101".to_owned(), 101),
                ("Test model #102".to_owned(), 102),
                ("Test model #103".to_owned(), 103),
                ("Test model #104".to_owned(), 104),
                ("Test model #105".to_owned(), 105),
                ("Test model #106".to_owned(), 106),
                ("Test model #107".to_owned(), 107),
                ("Test model #108".to_owned(), 108),
                ("Test model #109".to_owned(), 109),
                ("Test model #110".to_owned(), 110),
                ("Test model #111".to_owned(), 111),
                ("Test model #112".to_owned(), 112),
                ("Test model #113".to_owned(), 113),
                ("Test model #114".to_owned(), 114),
                ("Test model #115".to_owned(), 115),
                ("Test model #120".to_owned(), 120),
                ("Test model #121".to_owned(), 121),
                ("Test model #122".to_owned(), 122),
                ("Test model #123".to_owned(), 123),
                ("Test model #124".to_owned(), 124),
            ]
        );
    }
}
