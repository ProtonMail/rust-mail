use test_log::test;

use super::*;

#[test]
fn test_defragmentation() {
    let mut sut = TextIndex::default();
    sut.insert(
        EntryIndex(0),
        AttributeIndex(0),
        &vec![
            ["defragmentation"]
                .into_iter()
                .map(Box::from)
                .enumerate()
                .collect::<Vec<_>>()
                .into(),
        ],
    );
    sut.insert(
        EntryIndex(1),
        AttributeIndex(0),
        &EntryValues::from(vec![
            ["calcidation"]
                .into_iter()
                .map(Box::from)
                .enumerate()
                .collect::<Vec<_>>()
                .into(),
        ]),
    );

    insta::assert_debug_snapshot!(sut);
    assert!(!sut.remove(&[EntryIndex(0)].into()).is_empty());
    insta::assert_debug_snapshot!(sut);
    assert!(sut.remap(&[(EntryIndex(1), EntryIndex(0))].into_iter().collect()));
    insta::assert_debug_snapshot!(sut);
}

#[test]
fn test_reversing_the_index() {
    // The index shall contain whatever input given to it, so any undesired terms or characters shall be filtered out on input.
    // It preserves terms as they are given so likewise, any normalization must be done beforehand.
    // Here we test, that the index does indeed keep all the content in order and it can be recovered.
    //
    // The test_get_all_entries output might be a good intermediate for a synchronization format

    fn tokenize<'a>(input: impl IntoIterator<Item = &'a str>) -> EntryValues {
        let values = input.into_iter();
        values
            .map(|input| {
                input
                    .split_whitespace()
                    .map(Box::<str>::from)
                    .enumerate()
                    .collect::<Vec<_>>()
                    .into()
            })
            .collect::<Vec<_>>()
    }

    let input = tokenize([
        "It will be seen that this mere painstaking burrower and grub-worm of
        a poor devil of a Sub-Sub appears to have gone through the long
        Vaticans and street-stalls of the earth, picking up whatever random
        allusions to whales he could anyways find in any book whatsoever,
        sacred or profane. Therefore you must not, in every case at least,
        take the higgledy-piggledy whale statements, however authentic, in
        these extracts, for veritable gospel cetology. Far from it. As
        touching the ancient authors generally, as well as the poets here
        appearing, these extracts are solely valuable or entertaining, as
        affording a glancing bird’s eye view of what has been promiscuously
        said, thought, fancied, and sung of Leviathan, by many nations and
        generations, including our own.",
        "So fare thee well, poor devil of a Sub-Sub, whose commentator I am.
        Thou belongest to that hopeless, sallow tribe which no wine of this
        world will ever warm; and for whom even Pale Sherry would be too
        rosy-strong; but with whom one sometimes loves to sit, and feel
        poor-devilish, too; and grow convivial upon tears; and say to them
        bluntly, with full eyes and empty glasses, and in not altogether
        unpleasant sadness—Give it up, Sub-Subs! For by how much the more
        pains ye take to please the world, by so much the more shall ye for
        ever go thankless! Would that I could clear out Hampton Court and the
        Tuileries for ye! But gulp down your tears and hie aloft to the
        royal-mast with your hearts; for your friends who have gone before
        are clearing out the seven-storied heavens, and making refugees of
        long-pampered Gabriel, Michael, and Raphael, against your coming.
        Here ye strike but splintered hearts together—there, ye shall strike
        unsplinterable glasses!",
    ]);

    let mut index = TextIndex::default();

    index.insert(EntryIndex(0), AttributeIndex(0), &input);

    let entries = index.test_get_all_entries();

    insta::assert_debug_snapshot!(entries,@r#"
    {
        (
            EntryIndex(
                0,
            ),
            AttributeIndex(
                0,
            ),
            ValueIndex(
                0,
            ),
        ): "It will be seen that this mere painstaking burrower and grub-worm of a poor devil of a Sub-Sub appears to have gone through the long Vaticans and street-stalls of the earth, picking up whatever random allusions to whales he could anyways find in any book whatsoever, sacred or profane. Therefore you must not, in every case at least, take the higgledy-piggledy whale statements, however authentic, in these extracts, for veritable gospel cetology. Far from it. As touching the ancient authors generally, as well as the poets here appearing, these extracts are solely valuable or entertaining, as affording a glancing bird’s eye view of what has been promiscuously said, thought, fancied, and sung of Leviathan, by many nations and generations, including our own.",
        (
            EntryIndex(
                0,
            ),
            AttributeIndex(
                0,
            ),
            ValueIndex(
                1,
            ),
        ): "So fare thee well, poor devil of a Sub-Sub, whose commentator I am. Thou belongest to that hopeless, sallow tribe which no wine of this world will ever warm; and for whom even Pale Sherry would be too rosy-strong; but with whom one sometimes loves to sit, and feel poor-devilish, too; and grow convivial upon tears; and say to them bluntly, with full eyes and empty glasses, and in not altogether unpleasant sadness—Give it up, Sub-Subs! For by how much the more pains ye take to please the world, by so much the more shall ye for ever go thankless! Would that I could clear out Hampton Court and the Tuileries for ye! But gulp down your tears and hie aloft to the royal-mast with your hearts; for your friends who have gone before are clearing out the seven-storied heavens, and making refugees of long-pampered Gabriel, Michael, and Raphael, against your coming. Here ye strike but splintered hearts together—there, ye shall strike unsplinterable glasses!",
    }
    "#);
}

#[test]
fn test_read_stats() {
    let mut sut = TextIndex::default();
    let terms = vec![vec![(0, "obscure".into()), (15, "security".into())].into()];
    sut.insert(EntryIndex(0), AttributeIndex(0), &terms);
    let terms = vec![vec![(1, "insecurity".into()), (16, "secured".into())].into()];
    sut.insert(EntryIndex(1), AttributeIndex(0), &terms);

    insta::assert_debug_snapshot!(sut.stats, @r"
    Stats {
        sizes: {
            AttributeIndex(
                0,
            ): {
                EntryIndex(
                    0,
                ): (
                    15,
                    2,
                ),
                EntryIndex(
                    1,
                ): (
                    17,
                    2,
                ),
            },
        },
    }
    ");
}

#[test]
fn test_insert_remove() {
    let mut index = TextIndex::default();
    index.insert_token(
        EntryIndex(0),
        AttributeIndex(0),
        ValueIndex(0),
        TokenPosition(0),
        "hippo",
    );
    index.insert_token(
        EntryIndex(1000),
        AttributeIndex(0),
        ValueIndex(0),
        TokenPosition(0),
        "yes",
    );

    assert!(index.test_find_posting(
        "yes",
        EntryIndex(1000),
        AttributeIndex(0),
        ValueIndex(0),
        TokenPosition(0)
    ));

    assert!(index.test_find_posting(
        "hippo",
        EntryIndex(0),
        AttributeIndex(0),
        ValueIndex(0),
        TokenPosition(0)
    ));

    assert_eq!(
        index.remove_inner(&[EntryIndex(0)].into(), Some(AttributeIndex(0)),),
        vec![(
            EntryIndex(0),
            AttributeIndex(0),
            vec![vec![(0, "hippo".into())].into()]
        )]
    );

    println!("{index:#?}");

    assert!(index.test_find_posting(
        "yes",
        EntryIndex(1000),
        AttributeIndex(0),
        ValueIndex(0),
        TokenPosition(0)
    ));

    assert!(!index.test_find_posting(
        "hippo",
        EntryIndex(0),
        AttributeIndex(0),
        ValueIndex(0),
        TokenPosition(0)
    ));
}
