mod size_rolling_appender;

pub use size_rolling_appender::*;
use std::path::{Path, PathBuf};
use typed_builder::TypedBuilder;

const DEFAULT_MAX_LOG_SIZE: u64 = 5 * 1024 * 1024; // 5MB
const DEFAULT_ROTATION_COUNT: usize = 2;

pub type LogFileHeader = fn() -> String;
#[derive(Debug, Clone, TypedBuilder)]
pub struct Config {
    pub directory: PathBuf,
    pub name: String,
    #[builder(default = String::from("log"))]
    pub suffix: String,
    #[builder(default = DEFAULT_MAX_LOG_SIZE)]
    pub max_log_size: u64,
    #[builder(default = DEFAULT_ROTATION_COUNT)]
    pub max_rotation_count: usize,
    #[builder(default = empty_log_file_header)]
    pub header: LogFileHeader,
}

#[must_use]
pub fn empty_log_file_header() -> String {
    String::new()
}

impl Config {
    #[must_use]
    pub fn log_file_path(&self, roll_over_counter: usize) -> PathBuf {
        if roll_over_counter == 0 {
            self.directory.join(&self.name).with_extension(&self.suffix)
        } else {
            self.directory
                .join(format!("{}_{roll_over_counter}", self.name))
                .with_extension(&self.suffix)
        }
    }

    #[must_use]
    pub fn log_file_name(&self, roll_over_counter: usize) -> String {
        if roll_over_counter == 0 {
            format!("{}.{}", self.name, self.suffix)
        } else {
            format!("{}_{}.{}", self.name, roll_over_counter, self.suffix)
        }
    }

    /// Returns all expected log paths in newest to oldest order.
    #[must_use]
    pub fn log_file_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        for counter in 0..=self.max_rotation_count {
            let path = self.log_file_path(counter);
            paths.push(path);
        }
        paths
    }

    /// Returns all existing log paths in newest to oldest order.
    #[must_use]
    pub fn existing_log_file_paths(&self) -> Vec<PathBuf> {
        self.log_file_paths()
            .into_iter()
            .filter(|path| path.exists())
            .collect()
    }

    pub fn export_logs(&self, path: &Path) -> std::io::Result<usize> {
        let log_content = self.export_logs_into_vec()?;
        if log_content.is_empty() {
            return Ok(0);
        }
        std::fs::write(path, &log_content)?;
        Ok(log_content.len())
    }

    pub fn export_logs_into_vec(&self) -> std::io::Result<Vec<u8>> {
        let log_files = self.existing_log_file_paths();
        let mut content = Vec::new();
        for log_file in log_files.into_iter().rev() {
            let mut log_file = std::fs::OpenOptions::new()
                .read(true)
                .create(false)
                .open(log_file)?;
            std::io::copy(&mut log_file, &mut content)?;
        }
        Ok(content)
    }
}

pub struct LogService {
    config: Config,
}

impl LogService {
    #[must_use]
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    //TODO: would be nicer to return an tracing fmt layer
    pub fn create_logger(&self) -> std::io::Result<SizeRollingAppender> {
        SizeRollingAppender::new(self.config.clone())
    }

    #[must_use]
    pub fn log_paths(&self) -> Vec<PathBuf> {
        self.config.log_file_paths()
    }

    /// Exports all logs into a single file at `path`.
    ///
    /// Logs are written from oldest to newest.
    pub fn export_logs(&self, path: &Path) -> std::io::Result<usize> {
        self.config.export_logs(path)
    }

    /// Exports all logs into a byte array.
    ///
    /// Logs are written from oldest to newest.
    pub fn export_logs_into_vec(&self) -> std::io::Result<Vec<u8>> {
        self.config.export_logs_into_vec()
    }

    #[must_use]
    pub fn default_log_path(&self) -> PathBuf {
        self.config.log_file_path(0)
    }

    #[must_use]
    pub fn default_log_file_name(&self) -> String {
        self.config.log_file_name(0)
    }

    #[must_use]
    pub fn silence_muon_errors_evn_filter() -> &'static str {
        "muon::http=off,muon::dns=off,muon::rt=off,muon::client::middleware::auth=error,muon=info"
    }
}
