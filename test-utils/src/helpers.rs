use std::sync::Arc;
use tempdir::TempDir;

pub struct Helpers {
    tmp_dir: Option<Arc<TempDir>>,
}

impl Helpers {
    pub fn new() -> Self {
        Self { tmp_dir: None }
    }

    // Provide the temporary directory, initializing it only once with shared ownership
    pub fn provide_tmp_dir(&mut self, dir_name: &str) -> Arc<TempDir> {
        if self.tmp_dir.is_none() {
            self.tmp_dir = Some(Arc::new(
                TempDir::new(dir_name).expect("Failed to create temp dir"),
            ));
        }
        // Clone the Arc, allowing multiple references to the TempDir
        self.tmp_dir.as_ref().unwrap().clone()
    }
}

impl Default for Helpers {
    fn default() -> Self {
        Self::new()
    }
}
