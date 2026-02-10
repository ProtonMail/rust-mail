#![allow(clippy::expect_used)]
use std::collections::BTreeMap;

use proton_foundation_search::document::{Document, Value};
use proton_foundation_search::engine::{Engine, QueryEvent, WriteEvent};
use proton_foundation_search::query::stats::CollectionStats;
use proton_foundation_search::serialization::SerDes;
use proton_foundation_search::transaction::{LoadEvent, SaveEvent};
use test_log::test;
use tracing::info;

#[test]
fn tag_prefix_search() {
    let mut storage = BTreeMap::new();

    let sut = Engine::builder().build();

    let mut write = sut.write().expect("single writer");
    write
        .insert(
            Document::new("123")
                .with_attribute("text", Value::text("Hello! Killin' in"))
                .with_attribute("text", Value::text("the name of"))
                .with_attribute("int", 321)
                .with_attribute("int", 42)
                .with_attribute("bool", true)
                .with_attribute("tag", Value::tag("daring"))
                .with_attribute("tag", Value::tag("words:short:abc")),
        )
        .expect("doc 123");
    write
        .insert(
            Document::new("456")
                .with_attribute("text", Value::text("Hello! Like the wild ones,"))
                .with_attribute("text", Value::text(" we seek shelter"))
                .with_attribute("int", 654)
                .with_attribute("int", 42)
                .with_attribute("bool", false)
                .with_attribute("tag", Value::tag("longing"))
                .with_attribute("tag", Value::tag("words:short:xyz")),
        )
        .expect("doc 456");

    commit(&mut storage, write.commit());

    insta::assert_debug_snapshot!(
        storage
            .iter()
            .map(|(k, v)| (k, String::from_utf8(v.clone())))
            .collect::<BTreeMap<_, _>>()
    );

    let query_expr = "tag~'words:short:*'";

    let result = query(
        &storage,
        sut.query()
            .with_expression(query_expr.parse().expect("query"))
            .search(),
    )
    .collect::<Vec<_>>();

    insta::assert_debug_snapshot!(sut);

    insta::assert_debug_snapshot!(result, @r#"
    [
        (
            "123",
            "0.76",
        ),
        (
            "456",
            "0.76",
        ),
    ]
    "#);

    let query_expr2 = "tag='words:short:abc'";

    let result = query(
        &storage,
        sut.query()
            .with_expression(query_expr2.parse().expect("query"))
            .search(),
    )
    .collect::<Vec<_>>();

    insta::assert_debug_snapshot!(result, @r#"
    [
        (
            "123",
            "0.76",
        ),
    ]
    "#);
}

fn query(
    storage: &BTreeMap<Box<str>, Vec<u8>>,
    query: impl Iterator<Item = QueryEvent>,
) -> impl Iterator<Item = (Box<str>, Box<str>)> {
    let mut search_stats = CollectionStats::default();
    let mut found = query
        .filter_map(|event| match event {
            QueryEvent::Load(LoadEvent { name, send }) => {
                send(
                    &SerDes::Json,
                    storage.get(&name).cloned().unwrap_or_default(),
                )
                .expect("send");
                None
            }
            QueryEvent::Found(found) => Some(found),
            QueryEvent::Stats(stats) => {
                search_stats += stats;
                None
            }
        })
        .collect::<Vec<_>>();
    search_stats.update_all_scores(&mut found);
    found.sort();
    found.into_iter().map(|found| {
        (
            found.identifier().into(),
            format!("{}", found.score().round(3)).into_boxed_str(),
        )
    })
}

fn commit(storage: &mut BTreeMap<Box<str>, Vec<u8>>, write: impl Iterator<Item = WriteEvent>) {
    for event in write {
        match event {
            WriteEvent::Modified(identifier) => {
                info!(event = "modified", ?identifier);
            }
            WriteEvent::Save(SaveEvent { name, recv }) => {
                let recv = recv(&SerDes::Json).expect("recv");
                storage.insert(name, recv);
            }
            WriteEvent::Load(LoadEvent { name, send }) => send(
                &SerDes::Json,
                storage.get(&name).cloned().unwrap_or_default(),
            )
            .expect("send"),
        }
    }
}
