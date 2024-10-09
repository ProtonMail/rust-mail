use std::path::Path;

/// Create a new `Command` with the given args.
#[macro_export]
macro_rules! cmd {
    ($cmd:expr $(, $arg:expr)* $(,)?) => {{
        let mut cmd = ::std::process::Command::new($cmd);

        $(
            cmd.arg($arg);
        )*

        cmd
    }};
}

/// Create a new `Command` and run it.
#[macro_export]
macro_rules! run {
    ($cmd:expr $(, $arg:expr)* $(,)?) => {{
        match $crate::cmd!($cmd $(, $arg)*).status() {
            Ok(status) => if status.success() {
                Ok(())
            } else {
                Err(::anyhow::anyhow!("failed to run command: {status}"))
            },

            Err(err) => Err(::anyhow::anyhow!(err)),
        }
    }};
}

/// Create a new `Command` and run it with the given working directory.
#[macro_export]
macro_rules! run_in {
    ($dir:expr, $cmd:expr $(, $arg:expr)* $(,)?) => {{
        match $crate::cmd!($cmd $(, $arg)*).current_dir($dir).status() {
            Ok(status) => if status.success() {
                Ok(())
            } else {
                Err(::anyhow::anyhow!("failed to run command: {status}"))
            },

            Err(err) => Err(::anyhow::anyhow!(err)),
        }
    }};
}

/// Extension trait for `Path`.
pub trait PathExt: AsRef<Path> + Sized {
    fn remove_if_exists(self) -> std::io::Result<Self> {
        let this = self.as_ref();

        if this.exists() {
            std::fs::remove_dir_all(this)?;
        }

        Ok(self)
    }
}

impl<This: AsRef<Path>> PathExt for This {}
