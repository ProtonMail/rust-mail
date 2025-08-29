use crate::{CoreContextError, UserContext};
use anyhow::anyhow;
use async_zip::Compression;
use async_zip::ZipDateTime;
use async_zip::ZipEntryBuilder;
use async_zip::base::write::ZipFileWriter;
use chrono::DateTime;
use chrono::Utc;
use futures::io::AsyncWriteExt;
use proton_core_api::services::proton::PostReportBug;
use proton_core_api::services::proton::ProtonCore;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt},
};
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};
use tracing::info;

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

    pub additional_files: Vec<PathBuf>,
}

pub enum ClientType {
    Email = 1,
}

const MAX_LOG_FILE_SIZE: usize = 1024 * 1024 * 20; // 20 MB

const MAX_ADDITIONAL_FILE_SIZE: usize = 1024 * 1024 * 5; // 5MB

type ZippedFile = (String, Vec<u8>);

/// Report an issue functionality.
///
/// # Errors
///
/// When logs cannot be zipped or API request fail
///
#[tracing::instrument(level = "debug", skip_all)]
pub async fn report_an_issue(
    report: IssueReport,
    user_ctx: &UserContext,
) -> Result<(), CoreContextError> {
    let acc_details = user_ctx.account_details().await?;
    let email = acc_details.email;

    if email.is_empty() {
        tracing::error!("Email address in account details is empty cannot send the bug report");
        return Err(CoreContextError::Other(anyhow!(
            "Email address cannot be empty"
        )));
    }

    let username = if acc_details.name.is_empty() {
        email.clone()
    } else {
        acc_details.name
    };

    let mut zip = ReportFileZipper::new();
    if report.logs {
        let log_file_name = user_ctx.log_service().default_log_file_name();
        let ctx_arc = user_ctx.as_arc();
        let content =
            tokio::task::spawn_blocking(move || ctx_arc.log_service().export_logs_into_vec())
                .await??;
        if content.is_empty() {
            tracing::error!("Could not attach logs to bug report, empty or missing");
        } else {
            let content = if content.len() > MAX_LOG_FILE_SIZE {
                &content[(content.len() - MAX_LOG_FILE_SIZE)..]
            } else {
                content.as_slice()
            };
            zip.add_from_memory(log_file_name, content, Utc::now())
                .await?;
        }
    }

    for path in &report.additional_files {
        tracing::debug!("Attaching extra file: {}", path.display());
        zip.add_from_path(&path, MAX_ADDITIONAL_FILE_SIZE, Utc::now())
            .await?;
    }

    let logs = zip.finalize().await?;

    let payload = create_bug_report_payload(report, username, email, logs);

    user_ctx.session().post_report_bug(payload).await?;

    info!("Issue reported");

    Ok(())
}

/// Form payload mirroring Proton's bug report API.
///
fn create_bug_report_payload(
    report: IssueReport,
    username: String,
    email: String,
    logs: Option<ZippedFile>,
) -> PostReportBug {
    let mut description = format!("SUMMARY\n{}", report.summary);

    if !report.steps_to_reproduce.is_empty() {
        description = format!(
            "{}\n\nSTEPS TO REPRODUCE\n{}",
            description, report.steps_to_reproduce
        );
    }

    if !report.expected_result.is_empty() {
        description = format!(
            "{}\n\nEXPECTED RESULT\n{}",
            description, report.expected_result
        );
    }

    if !report.actual_result.is_empty() {
        description = format!("{}\n\nACTUAL RESULT\n{}", description, report.actual_result);
    }

    PostReportBug {
        os: report.operating_system,
        os_version: report.operating_system_version,
        client: report.client,
        client_version: report.client_version,
        client_type: report.client_type as u8,
        title: report.title,
        description,
        username,
        email,
        logs,
    }
}

struct ReportFileZipper {
    zip: ZipFileWriter<Compat<Cursor<Vec<u8>>>>,
    file_names: HashMap<String, usize>,
    num_entries: usize,
}

impl ReportFileZipper {
    fn new() -> Self {
        let cursor = Cursor::new(Vec::new());
        let compat_cursor = cursor.compat_write(); // Make it compatible with futures-io
        Self {
            zip: ZipFileWriter::new(compat_cursor),
            file_names: HashMap::new(),
            num_entries: 0,
        }
    }

    fn transform_file_name(&mut self, file_name: String) -> String {
        match self.file_names.entry(file_name) {
            Entry::Occupied(mut o) => {
                *o.get_mut() += 1;
                format!("{}_{}", o.key(), o.get())
            }
            Entry::Vacant(v) => {
                let file_name = v.key().clone();
                v.insert(0);
                file_name
            }
        }
    }

    async fn add_from_memory(
        &mut self,
        file_name: String,
        content: &[u8],
        dt: DateTime<Utc>,
    ) -> Result<(), CoreContextError> {
        let file_name = self.transform_file_name(file_name);
        self.add_from_memory_impl(file_name, content, dt).await
    }

    async fn add_from_memory_impl(
        &mut self,
        file_name: String,
        content: &[u8],
        dt: DateTime<Utc>,
    ) -> Result<(), CoreContextError> {
        let entry = ZipEntryBuilder::new(file_name.into(), Compression::Deflate)
            .last_modification_date(ZipDateTime::from_chrono(&dt))
            .unix_permissions(0o644);
        let mut entry_writer = self.zip.write_entry_stream(entry).await.map_err(|e| {
            CoreContextError::Other(anyhow!(
                "Could not create stream zip writer, details: `{e}`"
            ))
        })?;

        entry_writer.write_all(content).await.map_err(|e| {
            CoreContextError::Other(anyhow!("Could not write bytes to the zip, details: {e}"))
        })?;
        entry_writer.close().await.map_err(|e| {
            CoreContextError::Other(anyhow!("Could not close stream zip writer, details: `{e}`"))
        })?;

        self.num_entries += 1;
        Ok(())
    }

    async fn add_from_path(
        &mut self,
        path: impl AsRef<Path>,
        max_bytes: usize,
        dt: DateTime<Utc>,
    ) -> Result<(), CoreContextError> {
        let mut file = File::open(&path).await?;
        let metadata = file.metadata().await?;

        if !metadata.is_file() {
            return Err(CoreContextError::Other(anyhow!(
                "Provided path is not a file, method `zip_file_in_memory` requires a path which points to a single file"
            )));
        }

        let log_bytes = metadata.len();
        let mut data_buf: Vec<u8>;

        #[allow(clippy::cast_possible_truncation)] // we validate the max value
        if log_bytes > max_bytes as u64 {
            let offset = log_bytes - max_bytes as u64;
            file.seek(std::io::SeekFrom::Start(offset)).await?;
            data_buf = Vec::with_capacity(max_bytes);
        } else {
            data_buf = Vec::with_capacity(log_bytes as usize);
        }

        file.read_to_end(&mut data_buf).await?;

        let file_name = self.transform_file_name(
            path.as_ref()
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| {
                    CoreContextError::Other(anyhow!(
                        "Path is a file and file should have a name, impossible"
                    ))
                })?
                .to_owned(),
        );

        self.add_from_memory_impl(file_name, &data_buf, dt).await
    }

    async fn finalize(self) -> Result<Option<(String, Vec<u8>)>, CoreContextError> {
        if self.num_entries == 0 {
            return Ok(None);
        }
        let now = Utc::now();
        let out_name = format!("{}_issue_report.zip", now.format("%Y%m%dT%H%M%S_%f"));
        let zipped_bytes = self
            .zip
            .close()
            .await
            .map_err(|e| {
                CoreContextError::Other(anyhow!(
                    "Could not get written bytes from a writer, details: `{e}`"
                ))
            })?
            .into_inner()
            .into_inner();
        Ok(Some((out_name, zipped_bytes)))
    }
}
