use crate::adapter::ConfigAdapter;
use crate::model::{Category, ConfigItem, Tool};
use anyhow::Result;
use std::path::PathBuf;

/// Adapter for Codex (~/.codex/)
pub struct CodexAdapter;

const SYNCABLE_FILES: &[(&str, Category, bool)] = &[
    ("config.toml", Category::Settings, false),
    ("AGENTS.md", Category::Instructions, false),
];

const SYNCABLE_DIRS: &[(&str, &str, Category, bool)] = &[
    ("rules", ".rules", Category::Rules, false),
    ("skills", ".md", Category::Skills, false),
];

/// Directories to skip when scanning skills
const SKIP_DIRS: &[&str] = &[".system"];

impl ConfigAdapter for CodexAdapter {
    fn config_dir(&self) -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".codex"))
    }

    fn scan(&self) -> Result<Vec<ConfigItem>> {
        let root = match self.config_dir() {
            Some(d) if d.exists() => d,
            _ => return Ok(Vec::new()),
        };

        let mut items = Vec::new();

        for (rel, cat, device_specific) in SYNCABLE_FILES {
            let path = root.join(rel);
            if path.is_file() {
                if let Ok(item) =
                    super::read_config_item(Tool::Codex, &path, &root, *cat, *device_specific)
                {
                    items.push(item);
                }
            }
        }

        for (dir, ext, cat, device_specific) in SYNCABLE_DIRS {
            let dir_path = root.join(dir);
            if dir_path.is_dir() {
                super::scan_dir_recursive(
                    Tool::Codex,
                    &dir_path,
                    &root,
                    ext,
                    *cat,
                    *device_specific,
                    SKIP_DIRS,
                    &mut items,
                )?;
            }
        }

        Ok(items)
    }
}
