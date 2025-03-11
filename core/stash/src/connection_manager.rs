pub use rusqlite;
use rusqlite::{Connection, Error, OpenFlags};
use std::fmt;
use std::path::{Path, PathBuf};
use tempdir::TempDir;
use uuid::Uuid;

#[derive(Debug)]
enum Source {
    File(PathBuf),
    TmpFile(TempDir),
}

type InitFn = dyn Fn(&mut Connection) -> Result<(), rusqlite::Error> + Send + Sync + 'static;

/// An `r2d2::ManageConnection` for `rusqlite::Connection`s.
pub struct StashConnectionManager {
    source: Source,
    flags: OpenFlags,
    init: Option<Box<InitFn>>,
}

impl fmt::Debug for StashConnectionManager {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut builder = f.debug_struct("StashConnectionMananger");
        let _ = builder.field("source", &self.source);
        let _ = builder.field("flags", &self.source);
        let _ = builder.field("init", &self.init.as_ref().map(|_| "InitFn"));
        builder.finish()
    }
}

impl StashConnectionManager {
    /// Creates a new `StashConnectionMananger` from file.
    ///
    /// See `rusqlite::Connection::open`
    pub fn file<P: AsRef<Path>>(path: P) -> Self {
        Self {
            source: Source::File(path.as_ref().to_path_buf()),
            flags: OpenFlags::default(),
            init: None,
        }
    }

    /// Creates a new `StashConnectionMananger` pretending to be memory database.
    /// Due to many issues with shared_cache option and many more without, decision was made
    /// to build temp file databases and keep them alive in Manager context.
    /// This allows for flexibility of memory database and stability of file database in nice wrapping.
    /// Since the production usage is exclusively file database it is nice bonus to run all tests in the
    /// file.
    ///
    pub fn tmp_file() -> Self {
        let tmp_dir =
            TempDir::new(Uuid::new_v4().to_string().as_str()).expect("failed to create temp dir");

        Self {
            source: Source::TmpFile(tmp_dir),
            flags: OpenFlags::default(),
            init: None,
        }
    }

    /// Converts `StashConnectionMananger` into one that calls an initialization
    /// function upon connection creation. Could be used to set PRAGMAs, for
    /// example.
    ///
    pub fn with_init<F>(mut self, init: F) -> Self
    where
        F: Fn(&mut Connection) -> Result<(), rusqlite::Error> + Send + Sync + 'static,
    {
        self.init = Some(Box::new(init));
        self
    }
}

impl r2d2::ManageConnection for StashConnectionManager {
    type Connection = Connection;
    type Error = rusqlite::Error;

    fn connect(&self) -> Result<Connection, Error> {
        match self.source {
            Source::File(ref path) => Connection::open_with_flags(path, self.flags),
            Source::TmpFile(ref tmp) => {
                Connection::open_with_flags(tmp.path().join("test"), self.flags)
            }
        }
        .and_then(|mut c| match self.init {
            None => Ok(c),
            Some(ref init) => init(&mut c).map(|_| c),
        })
    }

    fn is_valid(&self, conn: &mut Connection) -> Result<(), Error> {
        conn.execute_batch("SELECT 1")
    }

    fn has_broken(&self, _: &mut Connection) -> bool {
        false
    }
}
