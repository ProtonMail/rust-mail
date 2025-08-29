use crate::Config;
use parking_lot::{RwLock, RwLockReadGuard, RwLockUpgradableReadGuard, RwLockWriteGuard};
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing_subscriber::fmt::MakeWriter;

pub struct SizeRollingAppender {
    config: Config,
    file: RwLock<LogFile>,
}

impl SizeRollingAppender {
    pub fn new(config: Config) -> std::io::Result<Self> {
        if !config.directory.is_dir() {
            return Err(std::io::Error::new(
                ErrorKind::NotADirectory,
                format!("Path is not a directory: {}", config.directory.display()),
            ));
        }
        let mut log_file = LogFile::new(&config)?;

        if log_file.should_rotate(&config)
            && let Err(e) = log_file.rotate(&config)
        {
            eprintln!("Failed to rotate log file: {e:?}");
        }
        Ok(Self {
            config,
            file: RwLock::new(log_file),
        })
    }
}

struct LogFile {
    file: File,
    written: AtomicU64,
}

impl LogFile {
    fn new(config: &Config) -> std::io::Result<Self> {
        let file_path = config.log_file_path(0);
        let file = new_file(&file_path)?;
        let file_size = file.metadata()?.size();
        let mut this = Self {
            file,
            written: AtomicU64::new(file_size),
        };
        this.apply_log_header(config)?;
        Ok(this)
    }

    fn should_rotate(&self, config: &Config) -> bool {
        self.written.load(Ordering::Acquire) >= config.max_log_size
    }

    fn apply_log_header(&mut self, config: &Config) -> std::io::Result<()> {
        let header = (config.header)();
        if !header.is_empty() {
            self.file.write_all(header.as_bytes())?;
            self.written
                .fetch_add(header.len() as u64, Ordering::Release);
        }
        Ok(())
    }

    fn rotate(&mut self, config: &Config) -> std::io::Result<()> {
        self.file.flush()?;
        let range = (0..config.max_rotation_count).rev();
        for rotation in range {
            let old_file = config.log_file_path(rotation);
            let renamed_file = config.log_file_path(rotation + 1);
            if old_file.exists() {
                std::fs::rename(old_file, renamed_file)?;
            }
        }
        let new_log_file = new_file(&config.log_file_path(0))?;
        self.file = new_log_file;
        self.written.store(0, Ordering::Release);
        self.apply_log_header(config)?;
        Ok(())
    }

    fn write(&self, buf: &[u8]) -> std::io::Result<usize> {
        let mut file = &self.file;
        let r = file.write(buf);
        if let Ok(written) = &r {
            self.written.fetch_add(*written as u64, Ordering::Release);
        }
        r
    }
}

fn new_file(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().append(true).create(true).open(path)
}

impl Write for SizeRollingAppender {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut log_file = self.file.write();
        if log_file.should_rotate(&self.config)
            && let Err(e) = log_file.rotate(&self.config)
        {
            eprintln!("Could not rotate files: {e:?}");
        }

        log_file.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.write().file.flush()
    }
}

pub struct SizeRollingAppenderWriter<'w>(RwLockReadGuard<'w, LogFile>);

impl<'a> MakeWriter<'a> for SizeRollingAppender {
    type Writer = SizeRollingAppenderWriter<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        let log_file = self.file.upgradable_read();
        let log_file = if log_file.should_rotate(&self.config) {
            let mut log_file = RwLockUpgradableReadGuard::upgrade(log_file);
            if let Err(e) = log_file.rotate(&self.config) {
                eprintln!("Failed to rotate files: {e:?}");
            }
            RwLockWriteGuard::downgrade(log_file)
        } else {
            RwLockUpgradableReadGuard::downgrade(log_file)
        };

        SizeRollingAppenderWriter(log_file)
    }
}

impl Write for SizeRollingAppenderWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let mut file = &self.0.file;
        file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;
    use std::path::PathBuf;
    use tempdir::TempDir;
    const MAX_SIZE: usize = 256;

    const BYTES_1: [u8; MAX_SIZE] = [1; MAX_SIZE];
    const BYTES_2: [u8; MAX_SIZE] = [2; MAX_SIZE];
    const BYTES_3: [u8; MAX_SIZE] = [3; MAX_SIZE];
    const BYTES_4: [u8; MAX_SIZE] = [4; MAX_SIZE];

    fn test_config(path: PathBuf) -> Config {
        Config::builder()
            .directory(path)
            .name("test".into())
            .max_log_size(MAX_SIZE as u64)
            .max_rotation_count(2)
            .build()
    }

    #[test]
    fn rotation_check() {
        let tmp_dir = TempDir::new("log_service").unwrap();
        let config = test_config(tmp_dir.path().to_path_buf());

        let rotation_0_file = config.log_file_path(0);
        let rotation_1_file = config.log_file_path(1);
        let rotation_2_file = config.log_file_path(2);
        let rotation_3_file = config.log_file_path(3);

        let mut logger = SizeRollingAppender::new(config).unwrap();
        assert!(rotation_0_file.exists());

        // write one max size value to the log, no rotation
        logger.write_all(&BYTES_1).unwrap();
        assert!(rotation_0_file.exists());
        assert!(!rotation_1_file.exists());

        // write another max size value to the log, should have been rotated
        logger.write_all(&BYTES_2).unwrap();
        assert!(rotation_0_file.exists());
        assert!(rotation_1_file.exists());
        assert_eq!(std::fs::read(&rotation_0_file).unwrap(), BYTES_2);
        assert_eq!(std::fs::read(&rotation_1_file).unwrap(), BYTES_1);

        // write another max size value to the log, should have been rotated
        logger.write_all(&BYTES_3).unwrap();
        assert!(rotation_0_file.exists());
        assert!(rotation_1_file.exists());
        assert!(rotation_2_file.exists());
        assert_eq!(std::fs::read(&rotation_0_file).unwrap(), BYTES_3);
        assert_eq!(std::fs::read(&rotation_1_file).unwrap(), BYTES_2);
        assert_eq!(std::fs::read(&rotation_2_file).unwrap(), BYTES_1);

        // write another max size value to the log, should have been rotated, but no
        // more entries are created.
        logger.write_all(&BYTES_4).unwrap();
        assert!(rotation_0_file.exists());
        assert!(rotation_1_file.exists());
        assert!(rotation_2_file.exists());
        assert!(!rotation_3_file.exists());
        assert_eq!(std::fs::read(&rotation_0_file).unwrap(), BYTES_4);
        assert_eq!(std::fs::read(&rotation_1_file).unwrap(), BYTES_3);
        assert_eq!(std::fs::read(&rotation_2_file).unwrap(), BYTES_2);
    }

    #[test]
    fn rotate_at_startup_at_limit() {
        let tmp_dir = TempDir::new("log_service").unwrap();
        let config = test_config(tmp_dir.path().to_path_buf());

        let rotation_0_file = config.log_file_path(0);
        let rotation_1_file = config.log_file_path(1);

        std::fs::write(&rotation_0_file, BYTES_1).unwrap();

        let _logger = SizeRollingAppender::new(config).unwrap();
        assert!(rotation_0_file.exists());
        assert!(rotation_1_file.exists());

        assert_eq!(std::fs::read(&rotation_0_file).unwrap(), vec![]);
        assert_eq!(std::fs::read(&rotation_1_file).unwrap(), BYTES_1);
    }

    #[test]
    fn skip_rotate_at_startup_when_file_not_at_limit() {
        let tmp_dir = TempDir::new("log_service").unwrap();
        let config = test_config(tmp_dir.path().to_path_buf());

        let rotation_0_file = config.log_file_path(0);
        let rotation_1_file = config.log_file_path(1);

        std::fs::write(&rotation_0_file, [10; MAX_SIZE / 2]).unwrap();
        let _logger = SizeRollingAppender::new(config).unwrap();
        assert!(rotation_0_file.exists());
        assert!(!rotation_1_file.exists());
    }

    #[test]
    fn log_header() {
        fn log_header() -> String {
            String::from("Test log header\n")
        }

        let tmp_dir = TempDir::new("log_service").unwrap();
        let config = Config::builder()
            .directory(tmp_dir.path().to_path_buf())
            .name("test".into())
            .max_log_size(MAX_SIZE as u64)
            .max_rotation_count(1)
            .header(log_header)
            .build();

        let rotation_0_file = config.log_file_path(0);
        let rotation_1_file = config.log_file_path(1);

        let mut logger = SizeRollingAppender::new(config).unwrap();

        logger.write_all(&BYTES_1).unwrap();
        logger.write_all(&BYTES_2).unwrap();

        let mut expected_file_0 = log_header().into_bytes();
        expected_file_0.extend_from_slice(&BYTES_2);
        let mut expected_file_1 = log_header().into_bytes();
        expected_file_1.extend_from_slice(&BYTES_1);
        assert_eq!(std::fs::read(&rotation_0_file).unwrap(), expected_file_0);
        assert_eq!(std::fs::read(&rotation_1_file).unwrap(), expected_file_1);
    }

    #[test]
    fn log_export() {
        let tmp_dir = TempDir::new("log_service").unwrap();
        let config = test_config(tmp_dir.path().to_path_buf());
        let mut logger = SizeRollingAppender::new(config.clone()).unwrap();

        let log_output = tmp_dir.path().join("export");

        let mut expected = BYTES_1.to_vec();

        logger.write_all(&BYTES_1).unwrap();
        config.export_logs(&log_output).unwrap();
        assert_eq!(std::fs::read(&log_output).unwrap(), expected);

        logger.write_all(&BYTES_2).unwrap();
        config.export_logs(&log_output).unwrap();
        expected.extend_from_slice(&BYTES_2);
        assert_eq!(std::fs::read(&log_output).unwrap(), expected);

        logger.write_all(&BYTES_3).unwrap();
        config.export_logs(&log_output).unwrap();
        expected.extend_from_slice(&BYTES_3);
        assert_eq!(std::fs::read(&log_output).unwrap(), expected);
    }
}
