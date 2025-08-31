use crate::Result;

pub struct Indexer {
    // TODO: Add fields
}

impl Default for Indexer {
    fn default() -> Self {
        Self::new()
    }
}

impl Indexer {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn index_file(&self, _path: &std::path::Path) -> Result<()> {
        // TODO: Implement
        Ok(())
    }
}
