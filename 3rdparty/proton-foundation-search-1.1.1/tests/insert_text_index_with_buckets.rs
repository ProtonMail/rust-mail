use std::collections::BTreeMap;

use proton_foundation_search::index::prelude::*;
use proton_foundation_search::index::text::TextIndex as TextIndexWithBuckets;
use search_internal_helper::index_test_util::{do_test_commit, format_cache, format_storage};
use test_log::test;

#[test]
fn insert() {
    let mut storage = BTreeMap::new();
    let mut cache = BTreeMap::new();
    let sut = TextIndexWithBuckets {
        // each transaction is written into a new bucket
        maximum_token_bucket_size: 0,
    };
    let write = sut.write(
        None,
        &[
            IndexStoreOperation::Insert(
                EntryIndex(1),
                AttributeIndex(2),
                vec![Some(vec![(3, "umos".into()), (4, "lumo".into())].into())].into(),
            ),
            IndexStoreOperation::Insert(
                EntryIndex(10),
                AttributeIndex(20),
                vec![Some(vec![(30, "plum".into())].into())].into(),
            ),
            IndexStoreOperation::Insert(
                EntryIndex(1000),
                AttributeIndex(200),
                vec![Some(vec![(3000, "lumi".into())].into())].into(),
            ),
        ],
    );
    let revision = do_test_commit(write, &mut storage, &mut cache).expect("revision");
    insta::assert_debug_snapshot!(revision,@"BB23B41F96EF5080930516D5DC2082F8");
    insta::assert_debug_snapshot!(sut, @r"
    TextIndex {
        maximum_token_bucket_size: 0,
    }
    ");
    insta::assert_snapshot!(format_cache(&cache), @r"
    BB23B41F96EF5080930516D5DC2082F8: text index - cached proton_foundation_search::index::text::text_with_buckets::content::Content
    DC6574ACC8E153BBBCD02433027D6C7B: token occurrences bucket 0 - cached proton_foundation_search::index::text::text_with_buckets::tokens::Tokens
    ");
    insta::assert_snapshot!(format_storage(&storage));

    // second transaction

    let write = sut.write(
        Some(revision),
        &[
            IndexStoreOperation::Remove(EntryIndex(1)),
            IndexStoreOperation::Insert(
                EntryIndex(100),
                AttributeIndex(200),
                vec![Some(
                    vec![(300, "fumo".into()), (400, "zumo".into())].into(),
                )]
                .into(),
            ),
            IndexStoreOperation::Insert(
                EntryIndex(10),
                AttributeIndex(20),
                vec![Some(vec![(30, "plums".into())].into())].into(),
            ),
        ],
    );
    let revision = do_test_commit(write, &mut storage, &mut cache).expect("revision");
    insta::assert_debug_snapshot!(revision,@"B410382150B35A3FB2061D19CA35C3DF");
    insta::assert_debug_snapshot!(sut, @r"
    TextIndex {
        maximum_token_bucket_size: 0,
    }
    ");
    insta::assert_snapshot!(format_cache(&cache), @r"
    BB23B41F96EF5080930516D5DC2082F8: released text index - cached proton_foundation_search::index::text::text_with_buckets::content::Content - cached ()
    DC6574ACC8E153BBBCD02433027D6C7B: released token occurrences bucket 0 - cached proton_foundation_search::index::text::text_with_buckets::tokens::Tokens - cached ()
    B410382150B35A3FB2061D19CA35C3DF: text index - cached proton_foundation_search::index::text::text_with_buckets::content::Content
    9F258BA04D1F56E9A5B95490FDF649C3: token occurrences bucket 0 - cached proton_foundation_search::index::text::text_with_buckets::tokens::Tokens
    BE614E38C42558EF8C3790E97A6070A9: token occurrences bucket 1 - cached proton_foundation_search::index::text::text_with_buckets::tokens::Tokens
    ");
    insta::assert_snapshot!(format_storage(&storage));
}

#[test]
fn insert_single_bucket() {
    let mut storage = BTreeMap::new();
    let mut cache = BTreeMap::new();
    let sut = TextIndexWithBuckets {
        // every transaction is written into the same bucket
        maximum_token_bucket_size: usize::MAX,
    };
    let write = sut.write(
        None,
        &[
            IndexStoreOperation::Insert(
                EntryIndex(1),
                AttributeIndex(2),
                vec![Some(vec![(3, "umos".into()), (4, "lumo".into())].into())].into(),
            ),
            IndexStoreOperation::Insert(
                EntryIndex(10),
                AttributeIndex(20),
                vec![Some(vec![(30, "plum".into())].into())].into(),
            ),
            IndexStoreOperation::Insert(
                EntryIndex(1000),
                AttributeIndex(200),
                vec![Some(vec![(3000, "lumi".into())].into())].into(),
            ),
        ],
    );
    let revision = do_test_commit(write, &mut storage, &mut cache).expect("revision");
    insta::assert_debug_snapshot!(revision,@"BB23B41F96EF5080930516D5DC2082F8");
    insta::assert_debug_snapshot!(sut, @r"
    TextIndex {
        maximum_token_bucket_size: 18446744073709551615,
    }
    ");
    insta::assert_snapshot!(format_cache(&cache), @r"
    BB23B41F96EF5080930516D5DC2082F8: text index - cached proton_foundation_search::index::text::text_with_buckets::content::Content
    DC6574ACC8E153BBBCD02433027D6C7B: token occurrences bucket 0 - cached proton_foundation_search::index::text::text_with_buckets::tokens::Tokens
    ");
    insta::assert_snapshot!(format_storage(&storage));

    // second transaction

    let write = sut.write(
        Some(revision),
        &[
            IndexStoreOperation::Remove(EntryIndex(1)),
            IndexStoreOperation::Insert(
                EntryIndex(100),
                AttributeIndex(200),
                vec![Some(
                    vec![(300, "fumo".into()), (400, "zumo".into())].into(),
                )]
                .into(),
            ),
            IndexStoreOperation::Insert(
                EntryIndex(10),
                AttributeIndex(20),
                vec![Some(vec![(30, "plums".into())].into())].into(),
            ),
        ],
    );
    let revision = do_test_commit(write, &mut storage, &mut cache).expect("revision");
    insta::assert_debug_snapshot!(revision, @"14062C6B8F7F52D4BCA46B6D11383E27");
    insta::assert_debug_snapshot!(sut, @r"
    TextIndex {
        maximum_token_bucket_size: 18446744073709551615,
    }
    ");
    insta::assert_snapshot!(format_cache(&cache), @r"
    BB23B41F96EF5080930516D5DC2082F8: released text index - cached proton_foundation_search::index::text::text_with_buckets::content::Content - cached ()
    DC6574ACC8E153BBBCD02433027D6C7B: released token occurrences bucket 0 - cached proton_foundation_search::index::text::text_with_buckets::tokens::Tokens - cached ()
    14062C6B8F7F52D4BCA46B6D11383E27: text index - cached proton_foundation_search::index::text::text_with_buckets::content::Content
    1106DF9FCB685C0082F9B8D3D1858F6F: token occurrences bucket 0 - cached proton_foundation_search::index::text::text_with_buckets::tokens::Tokens
    ");
    insta::assert_snapshot!(format_storage(&storage));
}
