use crate::adapter::ConfigAdapter;
use crate::model::{Category, ConfigItem, Tool};
use anyhow::Result;
use std::path::PathBuf;

/// Adapter for Claude Code (~/.claude/)
pub struct ClaudeCodeAdapter;

/// Syncable file definitions: (relative_path, category, is_device_specific)
const SYNCABLE_FILES: &[(&str, Category, bool)] = &[
    ("settings.json", Category::Settings, false),
    ("settings.local.json", Category::Settings, true),
    ("CLAUDE.md", Category::Instructions, false),
    ("CLAUDE.local.md", Category::Instructions, true),
    ("config.json", Category::Settings, true),
];

/// Syncable directories: (dir_path, file_extension, category, is_device_specific)
const SYNCABLE_DIRS: &[(&str, &str, Category, bool)] = &[
    ("commands", ".md", Category::Commands, false),
    ("skills", ".md", Category::Skills, false),
];

/// Plugin metadata files to sync
const PLUGIN_FILES: &[(&str, Category, bool)] = &[
    ("plugins/installed_plugins.json", Category::Plugins, false),
    ("plugins/known_marketplaces.json", Category::Plugins, false),
];

impl ConfigAdapter for ClaudeCodeAdapter {
    fn config_dir(&self) -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".claude"))
    }

    fn scan(&self) -> Result<Vec<ConfigItem>> {
        let root = match self.config_dir() {
            Some(d) if d.exists() => d,
            _ => return Ok(Vec::new()),
        };

        let mut items = Vec::new();

        // Scan individual files
        for (rel, cat, device_specific) in SYNCABLE_FILES.iter().chain(PLUGIN_FILES.iter()) {
            let path = root.join(rel);
            if path.is_file() {
                if let Ok(item) =
                    super::read_config_item(Tool::ClaudeCode, &path, &root, *cat, *device_specific)
                {
                    items.push(item);
                }
            }
        }

        // Scan directories
        for (dir, ext, cat, device_specific) in SYNCABLE_DIRS {
            let dir_path = root.join(dir);
            if dir_path.is_dir() {
                super::scan_dir_recursive(
                    Tool::ClaudeCode,
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
