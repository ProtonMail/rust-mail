use std::env;

use async_zip::base::read::mem::ZipFileReader;
use chrono::NaiveDate;
use futures::AsyncReadExt;
use tokio::fs;

use super::*;

#[tokio::test]
async fn zip_whole_file() {
    let current = env::current_dir().unwrap();
    let path = current.join("src/tests/issue_report/data/input.txt");
    let max_size = 1024 * 1024; // 1Mb
    let now = NaiveDate::from_ymd_opt(2025, 3, 20)
        .and_then(|day| day.and_hms_opt(12, 0, 0))
        .unwrap()
        .and_utc();
    let mut zipper = ReportFileZipper::new();
    zipper.add_from_path(&path, max_size, now).await.unwrap();
    let (_, contents) = zipper.finalize().await.unwrap().unwrap();
    let actual_file = unzip_file_in_memory(contents).await;
    let expected_file = fs::read(path).await.unwrap();
    assert_eq!(expected_file, actual_file);
}
#[tokio::test]
async fn zip_duplicates() {
    let current = env::current_dir().unwrap();
    let path = current.join("src/tests/issue_report/data/input.txt");
    let max_size = 1024 * 1024; // 1Mb
    let now = NaiveDate::from_ymd_opt(2025, 3, 20)
        .and_then(|day| day.and_hms_opt(12, 0, 0))
        .unwrap()
        .and_utc();
    let duplicate_content = [128_u8; 100];
    let mut zipper = ReportFileZipper::new();
    zipper.add_from_path(&path, max_size, now).await.unwrap();
    zipper
        .add_from_memory("input.txt".into(), &duplicate_content, Utc::now())
        .await
        .unwrap();
    let (_, contents) = zipper.finalize().await.unwrap().unwrap();
    let zip_reader = ZipFileReader::new(contents).await.unwrap();

    let mut entry_0 = zip_reader.reader_with_entry(0).await.unwrap();
    let mut entry_1 = zip_reader.reader_with_entry(1).await.unwrap();

    assert_eq!(entry_0.entry().filename().as_str().unwrap(), "input.txt");
    assert_eq!(entry_1.entry().filename().as_str().unwrap(), "input.txt_1",);

    let mut entry_0_contents = Vec::new();
    let mut entry_1_contents = Vec::new();
    entry_0.read_to_end(&mut entry_0_contents).await.unwrap();
    entry_1.read_to_end(&mut entry_1_contents).await.unwrap();

    let expected_file = fs::read_to_string(path).await.unwrap();
    assert_eq!(entry_0_contents, expected_file.as_bytes());
    assert_eq!(entry_1_contents, duplicate_content);
}

#[tokio::test]
async fn zip_last_paragraph() {
    let current = env::current_dir().unwrap();
    let path = current.join("src/tests/issue_report/data/input.txt");
    let max_size = 111; // last paragraph length
    let now = NaiveDate::from_ymd_opt(2025, 3, 20)
        .and_then(|day| day.and_hms_opt(12, 0, 0))
        .unwrap()
        .and_utc();
    let mut zipper = ReportFileZipper::new();
    zipper.add_from_path(&path, max_size, now).await.unwrap();
    let (_, contents) = zipper.finalize().await.unwrap().unwrap();
    let actual_file = unzip_file_in_memory(contents).await;
    let expected_file = fs::read_to_string(path).await.unwrap();
    let mut expected_paragraph = expected_file.lines().next_back().unwrap().to_string();

    // due to converting it to `lines` new empty line is missing at the end.
    expected_paragraph.push('\n');

    assert_eq!(expected_paragraph.as_bytes(), &actual_file);
}

#[tokio::test]
async fn empty_zip() {
    let zip = ReportFileZipper::new();
    let output = zip.finalize().await.unwrap();
    assert!(output.is_none());
}

async fn unzip_file_in_memory(file: Vec<u8>) -> Vec<u8> {
    let zip_reader = ZipFileReader::new(file).await.unwrap();
    let mut entry = zip_reader.reader_without_entry(0).await.unwrap();
    let mut data = Vec::new();
    entry.read_to_end(&mut data).await.unwrap();

    data
}
