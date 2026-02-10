use super::*;

#[test]
fn exports() {
    let mut sut = TextIndex::default();
    sut.insert(
        1.into(),
        0.into(),
        &vec![
            // handle overlaps
            vec![(0, "mighty".into()), (0, "textindex".into())].into(),
            // dump should preserve empty values
            vec![].into(),
            // handle token repetition
            vec![(0, "curioser".into()), (20, "curioser".into())].into(),
        ],
    );
    sut.insert(0.into(), 1.into(), &vec![vec![(111, "lone".into())].into()]);

    let export = sut.export().collect::<Vec<_>>();

    // note that the export is sorted by entry-attribute
    insta::assert_debug_snapshot!(export, @r#"
    [
        (
            EntryIndex(
                0,
            ),
            AttributeIndex(
                1,
            ),
            [
                Text(
                    [
                        (
                            111,
                            "lone",
                        ),
                    ],
                ),
            ],
        ),
        (
            EntryIndex(
                1,
            ),
            AttributeIndex(
                0,
            ),
            [
                Text(
                    [
                        (
                            0,
                            "mighty",
                        ),
                        (
                            0,
                            "textindex",
                        ),
                    ],
                ),
                Empty,
                Text(
                    [
                        (
                            0,
                            "curioser",
                        ),
                        (
                            20,
                            "curioser",
                        ),
                    ],
                ),
            ],
        ),
    ]
    "#);
}
