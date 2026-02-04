use test_log::test;

use super::*;

#[test]
fn should_build_processor() {
    let proc: TextProcessor = ProcessorConfig::default().build();
    assert_eq!(proc.min_length, 3);
    assert_eq!(proc.max_length, 20);

    let proc: TextProcessor = ProcessorConfig::default()
        .with_min_length(5)
        .with_max_length(10)
        .build();
    assert_eq!(proc.min_length, 5);
    assert_eq!(proc.max_length, 10);
}

#[test]
fn should_process_text() {
    let proc = TextProcessor {
        min_length: 3,
        max_length: 20,
        enable_emojis: true,
        stop_words: [].into(),
    };
    let res = proc.process_document_tokens("Hello World with SmAll Words");
    // plural because we don't use stemming/inflectors
    insta::assert_debug_snapshot!(
        res, @r#"
        [
            (
                0,
                "hello",
            ),
            (
                6,
                "world",
            ),
            (
                12,
                "with",
            ),
            (
                17,
                "small",
            ),
            (
                23,
                "words",
            ),
        ]
        "#
    );

    let res = proc.process_document_tokens(
        "This word will be too long pneumonoultramicroscopicsilicovolcanoconiosis",
    );
    insta::assert_debug_snapshot!(
        res, @r#"
        [
            (
                0,
                "this",
            ),
            (
                5,
                "word",
            ),
            (
                10,
                "will",
            ),
            (
                18,
                "too",
            ),
            (
                22,
                "long",
            ),
        ]
        "#
    );
}

#[test]
fn should_process_text_with_edge_lengths() {
    let proc = TextProcessor {
        min_length: 3,
        max_length: 5,
        enable_emojis: true,
        stop_words: [].into(),
    };
    let res = proc.process_document_tokens("ab abc abcd abcde abcdef");
    insta::assert_debug_snapshot!(
        res, @r#"
        [
            (
                3,
                "abc",
            ),
            (
                7,
                "abcd",
            ),
            (
                12,
                "abcde",
            ),
        ]
        "#
    );
}

#[test]
fn should_process_empty_text() {
    let proc = TextProcessor {
        min_length: 3,
        max_length: 20,
        enable_emojis: true,
        stop_words: [].into(),
    };
    let res = proc.process_document_tokens("");
    assert!(res.is_empty());
}

#[test]
fn should_process_text_with_no_valid_words() {
    let proc = TextProcessor {
        min_length: 3,
        max_length: 20,
        enable_emojis: true,
        stop_words: [].into(),
    };
    let res = proc.process_document_tokens("  !@#$%^&*()_+=-`~[]{}\\|;:'\",./<>?  ");
    assert_eq!(res, vec![]);

    let res = proc.process_document_tokens("a bb ccc dddd !@#$%^&*");
    insta::assert_debug_snapshot!(
            res, @r#"
        [
            (
                5,
                "ccc",
            ),
            (
                9,
                "dddd",
            ),
        ]
        "#);
}

#[test]
fn should_process_hyphens() {
    let proc = TextProcessor {
        min_length: 3,
        max_length: 20,
        enable_emojis: true,
        stop_words: [].into(),
    };
    let res = proc.process_document_tokens("Fromage dans grands-mères");
    insta::assert_debug_snapshot!(
            res, @r#"
        [
            (
                0,
                "fromage",
            ),
            (
                8,
                "dans",
            ),
            (
                13,
                "grands",
            ),
            (
                20,
                "mères",
            ),
        ]
        "#);
}

#[test]
fn config_should_add_stop_words() {
    let mut sut = ProcessorConfig::default();
    sut.add_stop_words(["a", "and", "the", "without", "came", "went"]);
    let p = sut.build();
    assert_eq!(
        p.stop_words,
        ["a", "and", "the", "without", "came", "went"]
            .into_iter()
            .map(Into::into)
            .collect()
    )
}

#[test]
fn config_should_add_stop_word() {
    let mut sut = ProcessorConfig::default();
    sut.add_stop_words(["a", "and", "the", "without", "went"]);
    sut.add_stop_word("came");
    let p = sut.build();
    assert_eq!(
        p.stop_words,
        ["a", "and", "the", "without", "came", "went"]
            .into_iter()
            .map(Into::into)
            .collect()
    )
}

#[test]
fn config_should_add_stop_word_wasm() {
    let mut sut = ProcessorConfig::default();
    sut.add_stop_words(["a", "and", "the", "without", "went"]);
    sut.add_stop_word_wasm("came".to_owned());
    let p = sut.build();
    assert_eq!(
        p.stop_words,
        ["a", "and", "the", "without", "came", "went"]
            .into_iter()
            .map(Into::into)
            .collect()
    )
}

#[test]
fn config_with_stop_word_wasm() {
    let sut = ProcessorConfig::default()
        .with_stop_words(["a", "and", "the", "without", "went"])
        .with_stop_word_wasm("came".to_owned());
    let p = sut.build();
    assert_eq!(
        p.stop_words,
        ["a", "and", "the", "without", "came", "went"]
            .into_iter()
            .map(Into::into)
            .collect()
    )
}

#[test]
fn config_with_stop_word() {
    let sut = ProcessorConfig::default()
        .with_stop_words(["a", "and", "the", "without", "went"])
        .with_stop_word("came");
    let p = sut.build();
    assert_eq!(
        p.stop_words,
        ["a", "and", "the", "without", "came", "went"]
            .into_iter()
            .map(Into::into)
            .collect()
    )
}

#[test]
fn config_with_stop_words() {
    let sut =
        ProcessorConfig::default().with_stop_words(["a", "and", "the", "without", "came", "went"]);
    let p = sut.build();
    assert_eq!(
        p.stop_words,
        ["a", "and", "the", "without", "came", "went"]
            .into_iter()
            .map(Into::into)
            .collect()
    )
}

#[test]
fn should_skip_stop_words() {
    let proc = TextProcessor {
        min_length: 3,
        max_length: 20,
        enable_emojis: true,
        stop_words: ["a", "and", "the", "without", "came", "went"]
            .into_iter()
            .map(Into::into)
            .collect(),
    };
    let res = proc.process_document_tokens("The fame came and went without trace");
    insta::assert_debug_snapshot!(
            res, @r#"
        [
            (
                4,
                "fame",
            ),
            (
                31,
                "trace",
            ),
        ]
        "#);
}
