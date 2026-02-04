use test_log::test;

use super::*;
use crate::query::expression::TermValue;

#[test]
fn joins_wildcards_when_not_tokenizing() {
    // preserves the sequence when parts are not split
    let processor = TextProcessor::default();
    let sut = TermValue::text("abcd").wildcard().then("efgh");
    let result = sut.process(&processor);
    assert_eq!(
        result,
        vec![TermValue::text("abcd").wildcard().then("efgh")]
    )
}

#[test]
fn joins_wildcards_when_tokenizing_wihtout_whitespace() {
    // preserves the sequence when parts are not split
    let processor = TextProcessor::default();
    let sut = TermValue::text("XXX abcd-").wildcard().then("-efgh XXX");
    let result = sut.process(&processor);
    assert_eq!(
        result,
        vec![
            TermValue::text("xxx"),
            TermValue::text("abcd").wildcard().then("efgh"),
            TermValue::text("xxx"),
        ]
    )
}

#[test]
fn separates_wildcards_with_whitespace() {
    // preserves the sequence when parts are not split
    let processor = TextProcessor::default();
    let sut = TermValue::text("abcd ").wildcard().then(" efgh");
    let result = sut.process(&processor);
    assert_eq!(
        result,
        vec![
            TermValue::text("abcd"),
            TermValue::wild(),
            TermValue::text("efgh")
        ]
    )
}

#[test]
fn joins_nontokenized_parts() {
    // preserves the sequence when parts are not split
    let processor = TextProcessor::default();
    let sut = TermValue::text("abcd").then("efgh");
    let result = sut.process(&processor);
    assert_eq!(result, vec![TermValue::text("abcdefgh")])
}

#[test]
fn does_not_exclude_short_words() {
    // preserves the sequence when parts are not split
    let processor = TextProcessor::default();
    let sut = TermValue::text("abc/de");
    let result = sut.process(&processor);
    assert_eq!(result, vec![TermValue::text("abc").wildcard().then("de")])
}

#[test]
fn tokenizes() {
    // preserves the sequence when parts are not split
    let processor = TextProcessor::default();
    let sut = TermValue::text("abcd efgh");
    let result = sut.process(&processor);
    assert_eq!(
        result,
        vec![TermValue::text("abcd"), TermValue::text("efgh")]
    )
}

#[test]
fn replaces_nontokens_with_wildcards() {
    // preserves the sequence when parts are not split
    let processor = TextProcessor::default();
    let sut = TermValue::text("abcd-efgh -other-");
    let result = sut.process(&processor);
    assert_eq!(
        result,
        vec![
            TermValue::text("abcd").wildcard().then("efgh"),
            TermValue::wild().then("other").wildcard()
        ]
    )
}

#[test]
fn ignores_insignificant_nonwhitespace() {
    // preserves the sequence when parts are not split
    let processor = TextProcessor::default();
    let sut = TermValue::text("- other - another -");
    let result = sut.process(&processor);
    assert_eq!(
        result,
        vec![TermValue::text("other"), TermValue::text("another")]
    )
}
