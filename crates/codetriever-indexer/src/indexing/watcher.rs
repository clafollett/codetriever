use crate::Result;

pub struct FileWatcher {
    // TODO: Add fields
}

impl Default for FileWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl FileWatcher {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn watch(&self, _path: &std::path::Path) -> Result<()> {
        // TODO: Implement
        Ok(())
    }
}
