//! Fixtures for these test are committed to the repository
//! to represent the point in time snapshot of blob structure
//! for a given version.
//!
//! Generate new fixtures with
//! ```sh
//! cargo run --example generate-compatibility-fixture JSON resources/compatibility/1.0/json
//! cargo run --example generate-compatibility-fixture CBOR resources/compatibility/1.0/cbor
//! ```
//! but do not update existing ones especially if they have been released publicly.
//!
//! Here we check that newer engine/index releases are able to digest older
//! release blobs. If this expectation is broken, we must add migrations
//! to ensure smooth upgrades of the search engine.
#![allow(clippy::expect_used)]

use std::path::Path;

use itertools::Itertools;
use proton_foundation_search::blob::LoadEvent;
use proton_foundation_search::engine::{Engine, QueryEvent};
use proton_foundation_search::query::stats::CollectionStats;
use proton_foundation_search::serialization::SerDes;

#[test]
fn compatibility_by_search() -> std::io::Result<()> {
    let versions: Vec<_> = std::fs::read_dir("../../resources/compatibility")?
        .filter_map_ok(|d| {
            if d.file_type().map(|t| t.is_dir()).unwrap_or_default() {
                Some(d.path())
            } else {
                None
            }
        })
        .collect::<Result<_, _>>()?;

    for version in versions {
        for serdes in [SerDes::Json, SerDes::Cbor] {
            let path = version.join(match serdes {
                SerDes::Json => "json",
                SerDes::JsonPretty => "json",
                SerDes::Cbor => "cbor",
            });
            let engine = Engine::builder().build();

            let results = [
                "privacy OR cool=* OR fool=*",
                "cool=true OR fool=false",
                "drive int>=2 tag=*",
                "tag=prime int=*",
            ]
            .into_iter()
            .map(|qy| {
                [
                    qy.to_owned(),
                    "\n======================\n".to_owned(),
                    search(
                        &serdes,
                        &path,
                        engine
                            .query()
                            .with_expression(qy.parse().expect("qy"))
                            .search(),
                    ),
                ]
                .concat()
            })
            .join("\n\n");

            let snapshot_name = version
                .file_name()
                .expect("version file name")
                .to_str()
                .expect("version file name str")
                .to_string();
            insta::assert_snapshot!(snapshot_name, results)
        }
    }

    Ok(())
}

fn search(
    serdes: &SerDes,
    path: impl AsRef<Path>,
    search: impl Iterator<Item = QueryEvent>,
) -> String {
    let mut stats = CollectionStats::default();
    let mut results = vec![];
    for event in search {
        match event {
            QueryEvent::Load(e) => load(serdes, &path, e),
            QueryEvent::Found(found_entry) => results.push(found_entry),
            QueryEvent::Stats(collection_stats) => stats += collection_stats,
        }
    }
    stats.update_all_scores(&mut results);
    results.sort();
    results
        .into_iter()
        .map(|f| {
            Some(format!("{} ", f.identifier()))
                .into_iter()
                .chain(f.matches().flat_map(|m| {
                    m.occurrences()
                        .into_iter()
                        .map(|o| format!("{}:{}, ", o.attribute(), m.value().to_string()))
                }))
                .collect::<String>()
        })
        .join("\n")
}

// fn commit(serdes: &SerDes, path: &str, write: impl Iterator<Item = WriteEvent>) {
//     for event in write {
//         match event {
//             WriteEvent::Save(e) => save(serdes, path, e),
//             WriteEvent::Load(e) => load(serdes, path, e),
//             WriteEvent::Stats(s) => {
//                 println!("Stats: {s:#?}");
//             }
//         }
//     }
// }

// fn cleanup(serdes: &SerDes, path: &str, cleanup: impl Iterator<Item = CleanupEvent>) {
//     for event in cleanup {
//         match event {
//             CleanupEvent::Release(e) => remove(path, e),
//             CleanupEvent::Load(e) => load(serdes, path, e),
//             CleanupEvent::Save(e) => save(serdes, path, e),
//         }
//     }
// }

// fn remove(path: &str, release: ReleaseEvent) {
//     let path = Path::new(path).join(release.id().to_string());
//     println!("Removing {path:?} for {release:?}");
//     std::fs::remove_file(path).expect("remove blob at {path:?}");
// }

// fn save(serdes: &SerDes, path: impl AsRef<Path>, save: SaveEvent) {
//     std::fs::create_dir_all(path.as_ref()).expect("storage dir cretion at {path:?}");
//     let path = path.as_ref().join(save.id().to_string());
//     let blob = save.recv();
//     println!("Saving {path:?} for {blob:?}");
//     let data = blob.serialize(serdes).expect("recv");
//     std::fs::write(path, data).expect("store blob at {path:?}");
// }

fn load(serdes: &SerDes, path: impl AsRef<Path>, load: LoadEvent) {
    let path = path.as_ref().join(load.id().to_string());
    println!("Loading {path:?} for {load:?}");
    if std::fs::exists(&path).expect("check if exists {path:?}") {
        let data = std::fs::read(path).expect("load blob at {path:?}");
        load.send(serdes, &data).expect("send");
    } else {
        load.send_empty().expect("send empty");
    }
}
