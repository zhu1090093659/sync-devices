mod claude_code;
mod codex;
mod cursor;
mod shared_agents;

use crate::model::{Category, ConfigItem, SyncManifest, Tool};
use crate::sanitizer;
use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Trait for tool-specific configuration adapters
pub trait ConfigAdapter {
    /// Returns the tool's config root directory
    fn config_dir(&self) -> Option<PathBuf>;

    /// Scan and collect all syncable config items
    fn scan(&self) -> Result<Vec<ConfigItem>>;
}

#[derive(Debug, Clone)]
pub struct LocalSnapshot {
    pub items: Vec<ConfigItem>,
    pub manifest: SyncManifest,
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
#[allow(clippy::too_many_arguments)]
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
            scan_dir_recursive(
                tool,
                &path,
                root,
                ext,
                category,
                is_device_specific,
                skip_dirs,
                items,
            )?;
        } else if path.extension().and_then(|e| e.to_str()) == Some(ext.trim_start_matches('.')) {
            if let Ok(item) = read_config_item(tool, &path, root, category, is_device_specific) {
                items.push(item);
            }
        }
    }

    Ok(())
}

/// Scan local config files and build a manifest aligned with the remote schema.
pub fn scan_local_manifest() -> Result<SyncManifest> {
    Ok(scan_local_snapshot()?.manifest)
}

/// Scan local config files and build a sanitized sync snapshot.
pub fn scan_local_snapshot() -> Result<LocalSnapshot> {
    let items = scan_all()?;
    build_local_snapshot(&items)
}

/// Build a local sync snapshot from scanned config items.
pub fn build_local_snapshot(items: &[ConfigItem]) -> Result<LocalSnapshot> {
    let generated_at = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let device_id = detect_device_id();
    Ok(build_local_snapshot_with_metadata(
        items,
        device_id,
        generated_at,
    ))
}

/// Resolve the local filesystem path for a sync item.
pub fn resolve_local_path(tool: Tool, rel_path: &str) -> Result<PathBuf> {
    let root = config_root(tool)
        .with_context(|| format!("missing config root directory for tool {}", tool.as_str()))?;
    resolve_local_path_from_root(&root, rel_path)
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

fn build_local_snapshot_with_metadata(
    items: &[ConfigItem],
    device_id: String,
    generated_at: u64,
) -> LocalSnapshot {
    let prepared_items = items
        .iter()
        .map(|item| prepare_sync_item(item, &device_id))
        .collect::<Vec<_>>();
    let manifest = SyncManifest::from_items(device_id, generated_at, &prepared_items);

    LocalSnapshot {
        items: prepared_items,
        manifest,
    }
}

fn prepare_sync_item(item: &ConfigItem, device_id: &str) -> ConfigItem {
    let redacted_content = sanitizer::redact(&item.content);
    let mut prepared = ConfigItem::new(
        item.tool,
        item.category,
        item.rel_path.clone(),
        redacted_content,
        item.last_modified,
        item.is_device_specific,
    );
    prepared.device_id = device_id.to_string();
    prepared
}

fn config_root(tool: Tool) -> Option<PathBuf> {
    dirs::home_dir().map(|home| match tool {
        Tool::ClaudeCode => home.join(".claude"),
        Tool::Codex => home.join(".codex"),
        Tool::Cursor => home.join(".cursor"),
        Tool::SharedAgents => home.join(".agents"),
    })
}

fn resolve_local_path_from_root(root: &Path, rel_path: &str) -> Result<PathBuf> {
    let mut resolved = root.to_path_buf();
    for segment in rel_path.split('/').filter(|segment| !segment.is_empty()) {
        if segment == "." || segment == ".." {
            return Err(anyhow!(
                "invalid relative path segment in sync item: {rel_path}"
            ));
        }
        resolved.push(segment);
    }

    if resolved == root {
        return Err(anyhow!("sync item path must not be empty"));
    }

    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn build_local_snapshot_redacts_sensitive_content_and_sets_device_id() {
        let items = vec![ConfigItem::new(
            Tool::Codex,
            Category::Settings,
            "config.toml".to_string(),
            "token = \"ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij\"".to_string(),
            42,
            false,
        )];

        let snapshot = build_local_snapshot_with_metadata(&items, "test-device".to_string(), 99);

        assert_eq!(snapshot.manifest.device_id, "test-device");
        assert_eq!(snapshot.manifest.generated_at, 99);
        assert_eq!(snapshot.items.len(), 1);
        assert_eq!(snapshot.items[0].device_id, "test-device");
        assert!(snapshot.items[0].content.contains("<REDACTED:github_pat>"));
        assert_ne!(snapshot.items[0].content_hash, items[0].content_hash);
        assert_eq!(
            snapshot.manifest.items[0].content_hash,
            snapshot.items[0].content_hash
        );
    }

    #[test]
    fn resolve_local_path_from_root_rejects_parent_segments() {
        let root = PathBuf::from("C:/temp/root");
        let error = resolve_local_path_from_root(&root, "../bad.txt").unwrap_err();

        assert!(error.to_string().contains("invalid relative path segment"));
    }

    #[test]
    fn resolve_local_path_from_root_rejects_dot_segment() {
        let root = PathBuf::from("C:/temp/root");
        let error = resolve_local_path_from_root(&root, "./bad.txt").unwrap_err();

        assert!(error.to_string().contains("invalid relative path segment"));
    }

    #[test]
    fn resolve_local_path_from_root_rejects_empty_path() {
        let root = PathBuf::from("C:/temp/root");
        let error = resolve_local_path_from_root(&root, "").unwrap_err();

        assert!(error.to_string().contains("must not be empty"));
    }

    #[test]
    fn resolve_local_path_from_root_builds_nested_path() {
        let root = PathBuf::from("C:/temp/root");
        let result = resolve_local_path_from_root(&root, "skills/my-skill/SKILL.md").unwrap();

        assert_eq!(
            result,
            PathBuf::from("C:/temp/root/skills/my-skill/SKILL.md")
        );
    }

    #[test]
    fn prepare_sync_item_redacts_and_stamps_device_id() {
        let raw = ConfigItem::new(
            Tool::ClaudeCode,
            Category::Settings,
            "settings.json".to_string(),
            "key = \"sk-abc123def456ghi789jkl012\"".to_string(),
            100,
            false,
        );
        let prepared = prepare_sync_item(&raw, "my-pc");

        assert_eq!(prepared.device_id, "my-pc");
        assert!(prepared.content.contains("<REDACTED:api_key>"));
        assert!(!prepared.content.contains("sk-abc123"));
    }

    #[test]
    fn config_root_returns_expected_paths_for_each_tool() {
        // config_root depends on home_dir which may vary, but the suffix is fixed
        if let Some(home) = dirs::home_dir() {
            assert_eq!(config_root(Tool::ClaudeCode).unwrap(), home.join(".claude"));
            assert_eq!(config_root(Tool::Codex).unwrap(), home.join(".codex"));
            assert_eq!(config_root(Tool::Cursor).unwrap(), home.join(".cursor"));
            assert_eq!(
                config_root(Tool::SharedAgents).unwrap(),
                home.join(".agents")
            );
        }
    }

    #[test]
    fn snapshot_manifest_entries_match_items() {
        let items = vec![
            ConfigItem::new(
                Tool::Codex,
                Category::Settings,
                "config.toml".to_string(),
                "clean content".to_string(),
                10,
                false,
            ),
            ConfigItem::new(
                Tool::ClaudeCode,
                Category::Commands,
                "commands/test.md".to_string(),
                "also clean".to_string(),
                20,
                true,
            ),
        ];

        let snapshot = build_local_snapshot_with_metadata(&items, "dev-1".to_string(), 50);

        assert_eq!(snapshot.items.len(), 2);
        assert_eq!(snapshot.manifest.items.len(), 2);
        // every manifest entry should have a corresponding item with matching hash
        for entry in &snapshot.manifest.items {
            let item = snapshot
                .items
                .iter()
                .find(|i| i.rel_path == entry.rel_path)
                .expect("manifest entry must have matching item");
            assert_eq!(item.content_hash, entry.content_hash);
            assert_eq!(item.device_id, entry.device_id);
        }
    }
}
