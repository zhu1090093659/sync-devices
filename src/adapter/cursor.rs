use crate::adapter::ConfigAdapter;
use crate::model::{Category, ConfigItem, Tool};
use anyhow::Result;
use std::path::PathBuf;

/// Adapter for Cursor (~/.cursor/)
pub struct CursorAdapter;

const SYNCABLE_FILES: &[(&str, Category, bool)] = &[("mcp.json", Category::Mcp, false)];

const SYNCABLE_DIRS: &[(&str, &str, Category, bool)] = &[
    ("commands", ".md", Category::Commands, false),
    ("rules", ".md", Category::Rules, false),
];

impl ConfigAdapter for CursorAdapter {
    fn config_dir(&self) -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".cursor"))
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
                    super::read_config_item(Tool::Cursor, &path, &root, *cat, *device_specific)
                {
                    items.push(item);
                }
            }
        }

        for (dir, ext, cat, device_specific) in SYNCABLE_DIRS {
            let dir_path = root.join(dir);
            if dir_path.is_dir() {
                super::scan_dir_recursive(
                    Tool::Cursor,
                    &dir_path,
                    &root,
                    ext,
                    *cat,
                    *device_specific,
                    &[],
                    &mut items,
                )?;
            }
        }

        Ok(items)
    }
}
