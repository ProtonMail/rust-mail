use std::env;

use chrono::NaiveDate;
use tokio::fs;

use super::zip_file_in_memory;

#[tokio::test]
async fn zip_whole_file() {
    let current = env::current_dir().unwrap();
    let path = current.join("src/tests/issue_report/data/input.txt");
    let bytes = 1024 * 1024; // 1Mb
    let now = NaiveDate::from_ymd_opt(2025, 3, 20)
        .and_then(|day| day.and_hms_opt(12, 0, 0))
        .unwrap()
        .and_utc();
    let (_, cursor) = zip_file_in_memory(&path, now, bytes).await.unwrap();
    let expected_file = fs::read(path.with_file_name("full_output_expected.zip"))
        .await
        .unwrap();
    assert_eq!(expected_file, cursor);
}

#[tokio::test]
async fn zip_last_paragraph() {
    let current = env::current_dir().unwrap();
    let path = current.join("src/tests/issue_report/data/input.txt");
    let bytes = 111; // last paragraph length
    let now = NaiveDate::from_ymd_opt(2025, 3, 20)
        .and_then(|day| day.and_hms_opt(12, 0, 0))
        .unwrap()
        .and_utc();
    let (_, cursor) = zip_file_in_memory(&path, now, bytes).await.unwrap();
    let expected_file = fs::read(path.with_file_name("last_paragraph_output_expected.zip"))
        .await
        .unwrap();
    assert_eq!(expected_file, cursor);
}
