mod claude_code;
mod codex;
mod cursor;
mod shared_agents;

use crate::model::{Category, ConfigItem, SyncManifest, Tool};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Trait for tool-specific configuration adapters
pub trait ConfigAdapter {
    /// Returns the tool's config root directory
    fn config_dir(&self) -> Option<PathBuf>;

    /// Scan and collect all syncable config items
    fn scan(&self) -> Result<Vec<ConfigItem>>;
}

/// Scan all supported tools and return a combined list of config items
pub fn scan_all() -> Result<Vec<ConfigItem>> {
    let adapters: Vec<Box<dyn ConfigAdapter>> = vec![
        Box::new(claude_code::ClaudeCodeAdapter),
        Box::new(codex::CodexAdapter),
        Box::new(cursor::CursorAdapter),
        Box::new(shared_agents::SharedAgentsAdapter),
    ];

    let mut all_items = Vec::new();
    for adapter in &adapters {
        match adapter.scan() {
            Ok(items) => all_items.extend(items),
            Err(e) => eprintln!("Warning: adapter scan failed: {}", e),
        }
    }

    Ok(all_items)
}

/// Read a config file and build a ConfigItem.
pub(crate) fn read_config_item(
    tool: Tool,
    path: &Path,
    root: &Path,
    category: Category,
    is_device_specific: bool,
) -> Result<ConfigItem> {
    let content = std::fs::read_to_string(path)?;
    let rel_path = path
        .strip_prefix(root)?
        .to_string_lossy()
        .replace('\\', "/");
    let modified = path
        .metadata()?
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    Ok(ConfigItem::new(
        tool,
        category,
        rel_path,
        content,
        modified,
        is_device_specific,
    ))
}

/// Recursively scan a directory for config files matching an extension.
pub(crate) fn scan_dir_recursive(
    tool: Tool,
    dir: &Path,
    root: &Path,
    ext: &str,
    category: Category,
    is_device_specific: bool,
    skip_dirs: &[&str],
    items: &mut Vec<ConfigItem>,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = match path.file_name() {
            Some(n) => n.to_string_lossy(),
            None => continue,
        };

        if name.starts_with('.') || skip_dirs.contains(&name.as_ref()) {
            continue;
        }

        if path.is_dir() {
            scan_dir_recursive(tool, &path, root, ext, category, is_device_specific, skip_dirs, items)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some(ext.trim_start_matches('.'))
        {
            if let Ok(item) = read_config_item(tool, &path, root, category, is_device_specific) {
                items.push(item);
            }
        }
    }

    Ok(())
}

/// Scan local config files and build a manifest aligned with the remote schema.
pub fn scan_local_manifest() -> Result<SyncManifest> {
    let items = scan_all()?;
    build_local_manifest(&items)
}

/// Build a local manifest from scanned config items.
pub fn build_local_manifest(items: &[ConfigItem]) -> Result<SyncManifest> {
    let generated_at = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    Ok(SyncManifest::from_items(
        detect_device_id(),
        generated_at,
        items,
    ))
}

fn detect_device_id() -> String {
    ["COMPUTERNAME", "HOSTNAME"]
        .into_iter()
        .find_map(read_non_empty_env)
        .unwrap_or_else(|| "unknown-device".to_string())
}

fn read_non_empty_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
