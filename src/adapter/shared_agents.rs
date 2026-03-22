use crate::adapter::ConfigAdapter;
use crate::model::{Category, ConfigItem, Tool};
use anyhow::Result;
use std::path::PathBuf;

/// Adapter for shared agents directory (~/.agents/)
pub struct SharedAgentsAdapter;

impl ConfigAdapter for SharedAgentsAdapter {
    fn config_dir(&self) -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".agents"))
    }

    fn scan(&self) -> Result<Vec<ConfigItem>> {
        let root = match self.config_dir() {
            Some(d) if d.exists() => d,
            _ => return Ok(Vec::new()),
        };

        let mut items = Vec::new();

        let skills_dir = root.join("skills");
        if skills_dir.is_dir() {
            super::scan_dir_recursive(
                Tool::SharedAgents,
                &skills_dir,
                &root,
                ".md",
                Category::Skills,
                false,
                &[],
                &mut items,
            )?;
        }

        Ok(items)
    }
}
