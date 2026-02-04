use test_log::test;

use super::*;
use crate::document::{Document, Value};
use crate::entry::Entry;

#[test]
fn getting_partition_value() {
    let title = "title";
    let creation = "creation";

    let processor = TextProcessor::default();

    let document = Document::new("foo.txt")
        .with_attribute(title, Value::text("Hello World"))
        .with_attribute(creation, 1234);
    let _processed = document.process(&processor);

    let document = Document::new("foo.txt").with_attribute(title, Value::text("Hello World"));
    let _processed = document.process(&processor);

    let document = Document::new("foo.txt")
        .with_attribute(title, Value::text("Hello World"))
        .with_attribute(creation, Value::text("bar"));
    let _processed = document.process(&processor);
}

#[test]
fn entry_partition_getter() {
    assert_eq!(Entry::new("foo", Default::default()).identifier(), "foo");
}

#[test]
fn can_add_more_than_65536_values_in_same_attribute() {
    let testfield = "testfield";
    let creation = "creation";

    let processor = TextProcessor::default();

    let mut document = Document::new("foo.txt");
    document.add_attribute(creation, u64::MAX);

    for i in 0..=u16::MAX {
        document.add_attribute(testfield, i as u64);
    }
    document.process(&processor);

    document.add_attribute(testfield, u64::MAX);

    document.process(&processor);
}
