#![allow(clippy::expect_used)]

use std::path::Path;

use proton_foundation_search::blob::{LoadEvent, SaveEvent};
use proton_foundation_search::document::{Document, Value};
use proton_foundation_search::engine::{Engine, WriteEvent};
use proton_foundation_search::serialization::SerDes;

/// Generate a fixture to check compatibility between version releases.
/// See packages/search/tests/compatibility.rs for actual tests.
fn main() {
    let mut args = std::env::args().skip(1);
    let serdes = match args
        .next()
        .expect("first arg SerDes - JSON or CBOR")
        .as_str()
    {
        "JSON" => SerDes::JsonPretty,
        "CBOR" => SerDes::Cbor,
        invalid => panic!("Invalid SerDes {invalid:?}"),
    };
    let dir = args.next().expect("second arg path to storage dir");
    let engine = Engine::builder().build();
    let mut write = engine.write().expect("write");
    write.insert(
        Document::new("defender")
            .with_attribute("text", Value::text("privacy defender"))
            .with_attribute("tag", Value::tag("prime"))
            .with_attribute("int", 1)
            .with_attribute("bool", true),
    );
    write.insert(
        Document::new("proton")
            .with_attribute("title", Value::text("Proton proposition"))
            .with_attribute("text", Value::text("protect privacy with Proton!"))
            .with_attribute("tag", Value::tag("prime"))
            .with_attribute("int", 2)
            .with_attribute("int", 3)
            .with_attribute("tool", true)
            .with_attribute("fool", false),
    );
    commit(&serdes, &dir, write.commit());

    let mut write = engine.write().expect("write");
    write.insert(
        Document::new("suite")
            .with_attribute("text", Value::text("vpn, mail, drive, docs, calendar"))
            .with_attribute("tag", Value::tag("gimme"))
            .with_attribute("int", 2)
            .with_attribute("bool", false),
    );
    write.insert(
        Document::new("nudge")
            .with_attribute("title", Value::text("Drive back your privacy back home!"))
            .with_attribute("text", Value::text("--redacted--"))
            .with_attribute("tag", Value::tag("gimme"))
            .with_attribute("int", 3)
            .with_attribute("int", 4)
            .with_attribute("cool", true)
            .with_attribute("fool", false),
    );
    commit(&serdes, &dir, write.commit());

    // not cleaning up so that this, too, can be tested
}

fn commit(serdes: &SerDes, path: &str, write: impl Iterator<Item = WriteEvent>) {
    for event in write {
        match event {
            WriteEvent::Save(e) => save(serdes, path, e),
            WriteEvent::Load(e) => load(serdes, path, e),
            WriteEvent::Stats(s) => {
                println!("Stats: {s:#?}");
            }
        }
    }
}

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

fn save(serdes: &SerDes, path: &str, save: SaveEvent) {
    std::fs::create_dir_all(path).expect("storage dir cretion at {path:?}");
    let path = Path::new(path).join(save.id().to_string());
    let blob = save.recv();
    println!("Saving {path:?} for {blob:?}");
    let data = blob.serialize(serdes).expect("recv");
    std::fs::write(path, data).expect("store blob at {path:?}");
}

fn load(serdes: &SerDes, path: &str, load: LoadEvent) {
    let path = Path::new(path).join(load.id().to_string());
    println!("Loading {path:?} for {load:?}");
    if std::fs::exists(&path).expect("check if exists {path:?}") {
        let data = std::fs::read(path).expect("load blob at {path:?}");
        load.send(serdes, &data).expect("send");
    } else {
        load.send_empty().expect("send empty");
    }
}
