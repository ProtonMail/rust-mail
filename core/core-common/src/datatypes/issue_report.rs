use anyhow::anyhow;
use async_zip::Compression;
use async_zip::ZipDateTime;
use async_zip::ZipEntryBuilder;
use async_zip::base::write::ZipFileWriter;
use chrono::DateTime;
use chrono::Utc;
use futures::io::AsyncWriteExt;
use proton_api_core::services::proton::PostReportBug;
use proton_api_core::services::proton::ProtonCore;
use proton_api_core::session::CoreSession;
use std::io::Cursor;
use std::path::Path;
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt},
};
use tokio_util::compat::TokioAsyncWriteCompatExt;

use crate::{CoreContextError, UserContext};

#[cfg(test)]
#[path = "../tests/issue_report/zip_file.rs"]
mod zip_file;

/// Represents Report issue form.
pub struct IssueReport {
    /// Name of the operating system app was run in.
    ///
    /// Provided by the client.
    ///
    /// # Example
    ///
    /// `iOS - iPhone`
    pub operating_system: String,

    /// Vesion of the operating system installed on the device.
    ///
    /// # Example
    ///
    /// `18.4`
    pub operating_system_version: String,

    /// Name of the client
    ///
    /// Provided by the client.
    ///
    /// # Example
    ///
    /// `iOS_Native`
    pub client: String,

    /// Version of the client application
    ///
    /// It is not verified but Semantic Versioning is encouraged.
    /// Provided by the client.
    ///
    /// # Example
    ///
    ///  `4.20.0`
    pub client_version: String,

    /// Type of client application
    ///
    /// Provided by the client.
    ///
    /// # Example
    ///
    /// `1` - Email
    pub client_type: ClientType,

    /// Common title for the client.
    ///
    /// Provided by the client.
    ///
    /// # Example
    ///
    /// `Proton Mail App bug report`
    pub title: String,

    /// Summary of the stumbled upon issue.
    ///
    /// The string has to be at least 10 characters long.
    /// Depicts incident, it is provided by the user.
    pub summary: String,

    /// The steps needed to reproduce the issue.
    ///
    /// Can be empty.
    /// Provided by the user.
    pub steps_to_reproduce: String,

    /// User's expected behavior.
    ///
    /// Can be empty.
    /// Provided by the user.
    pub expected_result: String,

    /// What happened instead.
    ///
    /// Can be empty.
    /// Provided by the user.
    pub actual_result: String,

    /// Permission to attach the logs to the report.
    ///
    /// User gave permission to share the logs with bug report
    /// by selecting an option in the client app.
    pub logs: bool,
}

pub enum ClientType {
    Email = 1,
}

/// Maximum number of bytes accepted - 50Mb
const MAX_LOG_BYTES: u64 = 1024 * 1024 * 50;
type ZippedFile = (String, Vec<u8>);

/// Report an issue functionality.
///
/// # Parameters
///
/// * `report` - representation of the form filled in by user,
/// * `user_ctx` - need for request making
///
/// # Errors
///
/// When logs cannot be zipped or API request fail
///
pub async fn report_an_issue(
    report: IssueReport,
    user_ctx: &UserContext,
) -> Result<(), CoreContextError> {
    let logs: Option<ZippedFile> = if report.logs {
        if let Some(log_path) = user_ctx.get_log_path() {
            Some(zip_file_in_memory(log_path, Utc::now(), MAX_LOG_BYTES).await?)
        } else {
            tracing::error!(
                "Could not attach logs to the bug report due to missing path in user context."
            );
            None
        }
    } else {
        None
    };
    let email = user_ctx.account_details().await?.email;

    if email.is_empty() {
        tracing::error!("Email address in account details is empty cannot send the bug report");
        return Err(CoreContextError::Other(anyhow!(
            "Email address cannot be empty"
        )));
    }

    let payload = create_bug_report_payload(report, email, logs);

    user_ctx.session().api().post_report_bug(payload).await?;

    Ok(())
}

/// Form payload mirroring Proton's bug report API.
///
fn create_bug_report_payload(
    report: IssueReport,
    email: String,
    logs: Option<ZippedFile>,
) -> PostReportBug {
    let mut description = format!("SUMMARY\n{}", report.summary);

    if !report.steps_to_reproduce.is_empty() {
        description = format!("\n\nSTEPS TO REPRODUCE\n{}", report.steps_to_reproduce);
    }

    if !report.expected_result.is_empty() {
        description = format!("\n\nEXPECTED RESULT\n{}", report.expected_result);
    }

    if !report.actual_result.is_empty() {
        description = format!("\n\nACTUAL RESULT\n{}", report.actual_result);
    }

    PostReportBug {
        os: report.operating_system,
        os_version: report.operating_system_version,
        client: report.client,
        client_version: report.client_version,
        client_type: report.client_type as u8,
        title: report.title,
        description,
        username: String::new(),
        email,
        logs,
    }
}

/// Zip file in memory
///
/// This function is meant to read & zip single file returning bytes ready to send.
///
/// # Parameters
///
/// * `path` - path to the file
/// * `now` - the current time in Utc, used for file creation time & as a prefix for zipped file name.
/// * `max_bytes` - how many bytes should be written to the zip if the file size exceeds the `max_byte` value.
///               Value is not verified but it is not recomended exceeding `MAX_LOG_BYTES` value as 50 Mb.
///
/// # Returns
///
/// Tuple of `FileName` (String) & `ZipBytes` (Vec<u8>)
///
/// # Errors
///
/// When IO fails. Most probable issue to encounter is by misusage of `path` parameter as it accepts single file paths only.
///
#[allow(clippy::cast_possible_truncation)]
async fn zip_file_in_memory(
    path: impl AsRef<Path>,
    now: DateTime<Utc>,
    max_bytes: u64,
) -> Result<ZippedFile, CoreContextError> {
    let mut file = File::open(&path).await?;
    let metadata = file.metadata().await?;

    if !metadata.is_file() {
        return Err(CoreContextError::Other(anyhow!(
            "Provided path is not a file, method `zip_file_in_memory` requires a path which points to a single file"
        )));
    }

    let log_bytes = metadata.len();
    let mut data_buf: Vec<u8>;

    if log_bytes > max_bytes {
        let offset = log_bytes - max_bytes;
        file.seek(std::io::SeekFrom::Start(offset)).await?;
        data_buf = Vec::with_capacity(max_bytes as usize);
    } else {
        data_buf = Vec::with_capacity(log_bytes as usize);
    }

    file.read_to_end(&mut data_buf).await?;

    let file_name = path
        .as_ref()
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            CoreContextError::Other(anyhow!(
                "Path is a file and file should have a name, impossible"
            ))
        })?;
    let out_name = format!("{}_{file_name}", now.format("%Y%m%dT%H%M%S_%f"));
    let cursor = Cursor::new(Vec::new());
    let compat_cursor = cursor.compat_write(); // Make it compatible with futures-io
    let mut zip_writer = ZipFileWriter::new(compat_cursor);
    let entry = ZipEntryBuilder::new(out_name.as_str().into(), Compression::Deflate)
        .last_modification_date(ZipDateTime::from_chrono(&now));
    let mut entry_writer = zip_writer.write_entry_stream(entry).await.map_err(|e| {
        CoreContextError::Other(anyhow!(
            "Could not create stream zip writer, details: `{e}`"
        ))
    })?;

    entry_writer.write_all(&data_buf).await.map_err(|e| {
        CoreContextError::Other(anyhow!("Could not write bytes to the zip, details: {e}"))
    })?;
    entry_writer.close().await.map_err(|e| {
        CoreContextError::Other(anyhow!("Could not close stream zip writer, details: `{e}`"))
    })?;

    let zipped_bytes = zip_writer
        .close()
        .await
        .map_err(|e| {
            CoreContextError::Other(anyhow!(
                "Could not get written bytes from a writer, details: `{e}`"
            ))
        })?
        .into_inner()
        .into_inner();

    Ok((out_name, zipped_bytes))
}
