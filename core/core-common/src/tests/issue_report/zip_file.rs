use std::env;

use async_zip::base::read::mem::ZipFileReader;
use chrono::NaiveDate;
use futures::AsyncReadExt;
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
    let actual_file = unzip_file_in_memory(cursor).await;
    let expected_file = fs::read(path).await.unwrap();

    assert_eq!(expected_file, actual_file);
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
    let actual_file = unzip_file_in_memory(cursor).await;
    let expected_file = fs::read_to_string(path).await.unwrap();
    let mut expected_paragraph = expected_file.lines().next_back().unwrap().to_string();

    // due to converting it to `lines` new empty line is missing at the end.
    expected_paragraph.push('\n');

    assert_eq!(expected_paragraph.as_bytes(), &actual_file);
}

async fn unzip_file_in_memory(file: Vec<u8>) -> Vec<u8> {
    let zip_reader = ZipFileReader::new(file).await.unwrap();
    let mut entry = zip_reader.reader_without_entry(0).await.unwrap();
    let mut data = Vec::new();
    entry.read_to_end(&mut data).await.unwrap();

    data
}
