use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Supported AI CLI tools
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    ClaudeCode,
    Codex,
    Cursor,
    SharedAgents,
}

/// Configuration item categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Settings,
    Instructions,
    Commands,
    Skills,
    Mcp,
    Plugins,
    Rules,
}

impl Tool {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude_code",
            Self::Codex => "codex",
            Self::Cursor => "cursor",
            Self::SharedAgents => "shared_agents",
        }
    }
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Settings => "settings",
            Self::Instructions => "instructions",
            Self::Commands => "commands",
            Self::Skills => "skills",
            Self::Mcp => "mcp",
            Self::Plugins => "plugins",
            Self::Rules => "rules",
        }
    }
}

/// A single syncable configuration item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigItem {
    pub tool: Tool,
    pub category: Category,
    /// Relative path within the tool's config directory
    pub rel_path: String,
    /// File content
    pub content: String,
    /// SHA-256 hash of content
    pub content_hash: String,
    /// Last modified timestamp (unix seconds)
    pub last_modified: u64,
    /// Device identifier that last modified this item
    pub device_id: String,
    /// Whether this item contains device-specific data (paths, env vars)
    pub is_device_specific: bool,
}

impl ConfigItem {
    /// Create a new ConfigItem, automatically computing the content hash
    pub fn new(
        tool: Tool,
        category: Category,
        rel_path: String,
        content: String,
        last_modified: u64,
        is_device_specific: bool,
    ) -> Self {
        let content_hash = compute_hash(&content);
        Self {
            tool,
            category,
            rel_path,
            content,
            content_hash,
            last_modified,
            device_id: String::new(),
            is_device_specific,
        }
    }
}

/// Sync manifest: a snapshot of all config items on a device or remote
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncManifest {
    /// Device name/identifier
    pub device_id: String,
    /// Timestamp when this manifest was generated
    pub generated_at: u64,
    /// All config item metadata (without full content, for diffing)
    pub items: Vec<ManifestEntry>,
}

impl SyncManifest {
    /// Build a manifest from scanned config items using a stable sort order.
    pub fn from_items(device_id: String, generated_at: u64, items: &[ConfigItem]) -> Self {
        let mut entries: Vec<_> = items.iter().map(ManifestEntry::from).collect();
        entries.sort_by(|left, right| {
            left.tool
                .as_str()
                .cmp(right.tool.as_str())
                .then(left.category.as_str().cmp(right.category.as_str()))
                .then(left.rel_path.cmp(&right.rel_path))
        });

        Self {
            device_id,
            generated_at,
            items: entries,
        }
    }
}

/// Lightweight entry in a manifest (no content, just metadata + hash)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub tool: Tool,
    pub category: Category,
    pub rel_path: String,
    pub content_hash: String,
    pub last_modified: u64,
    pub is_device_specific: bool,
}

impl From<&ConfigItem> for ManifestEntry {
    fn from(item: &ConfigItem) -> Self {
        Self {
            tool: item.tool,
            category: item.category,
            rel_path: item.rel_path.clone(),
            content_hash: item.content_hash.clone(),
            last_modified: item.last_modified,
            is_device_specific: item.is_device_specific,
        }
    }
}

/// Diff status between local and remote
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffStatus {
    /// Only exists locally
    LocalOnly,
    /// Only exists remotely
    RemoteOnly,
    /// Exists on both but content differs
    Modified,
    /// Both sides modified since last sync
    Conflict,
    /// Identical on both sides
    Unchanged,
}

/// A single diff result between local and remote manifest entries.
#[derive(Debug, Clone)]
pub struct ManifestDiffEntry {
    pub tool: Tool,
    pub category: Category,
    pub rel_path: String,
    pub local: Option<ManifestEntry>,
    pub remote: Option<ManifestEntry>,
    pub status: DiffStatus,
}

/// Aggregated counts for manifest diff output.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ManifestDiffSummary {
    pub local_only: usize,
    pub remote_only: usize,
    pub modified: usize,
    pub conflict: usize,
    pub unchanged: usize,
}

/// Compare local and remote manifests using tool/category/path identity plus metadata.
pub fn diff_manifests(local: &SyncManifest, remote: &SyncManifest) -> Vec<ManifestDiffEntry> {
    type Key<'a> = (&'a str, &'a str, &'a str);

    let local_map = index_manifest_entries(&local.items);
    let remote_map = index_manifest_entries(&remote.items);
    let keys: BTreeSet<Key> = local_map
        .keys()
        .chain(remote_map.keys())
        .copied()
        .collect();

    let mut entries = Vec::with_capacity(keys.len());
    for key in keys {
        let local_entry = local_map.get(&key).copied();
        let remote_entry = remote_map.get(&key).copied();
        let reference = local_entry
            .or(remote_entry)
            .expect("diff key must exist in at least one manifest");
        let status = match (local_entry, remote_entry) {
            (Some(_), None) => DiffStatus::LocalOnly,
            (None, Some(_)) => DiffStatus::RemoteOnly,
            (Some(l), Some(r)) if is_same_manifest_entry(l, r) => DiffStatus::Unchanged,
            (Some(_), Some(_)) => DiffStatus::Modified,
            (None, None) => unreachable!("diff key must exist in at least one manifest"),
        };

        entries.push(ManifestDiffEntry {
            tool: reference.tool,
            category: reference.category,
            rel_path: reference.rel_path.clone(),
            local: local_entry.cloned(),
            remote: remote_entry.cloned(),
            status,
        });
    }

    entries
}

/// Summarize manifest diff counts for status output.
pub fn summarize_manifest_diff(entries: &[ManifestDiffEntry]) -> ManifestDiffSummary {
    let mut summary = ManifestDiffSummary::default();
    for entry in entries {
        match entry.status {
            DiffStatus::LocalOnly => summary.local_only += 1,
            DiffStatus::RemoteOnly => summary.remote_only += 1,
            DiffStatus::Modified => summary.modified += 1,
            DiffStatus::Conflict => summary.conflict += 1,
            DiffStatus::Unchanged => summary.unchanged += 1,
        }
    }

    summary
}

fn compute_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn index_manifest_entries(
    items: &[ManifestEntry],
) -> BTreeMap<(&str, &str, &str), &ManifestEntry> {
    items
        .iter()
        .map(|item| {
            (
                (item.tool.as_str(), item.category.as_str(), item.rel_path.as_str()),
                item,
            )
        })
        .collect()
}

fn is_same_manifest_entry(left: &ManifestEntry, right: &ManifestEntry) -> bool {
    left.content_hash == right.content_hash && left.is_device_specific == right.is_device_specific
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_from_items_sorts_entries_and_preserves_metadata() {
        let items = vec![
            ConfigItem::new(
                Tool::Codex,
                Category::Rules,
                "rules/z.rules".to_string(),
                "z".to_string(),
                30,
                false,
            ),
            ConfigItem::new(
                Tool::ClaudeCode,
                Category::Commands,
                "commands/a.md".to_string(),
                "a".to_string(),
                10,
                false,
            ),
            ConfigItem::new(
                Tool::Codex,
                Category::Rules,
                "rules/a.rules".to_string(),
                "b".to_string(),
                20,
                true,
            ),
        ];

        let manifest = SyncManifest::from_items("test-device".to_string(), 123, &items);
        let ordered_paths: Vec<_> = manifest
            .items
            .iter()
            .map(|item| item.rel_path.as_str())
            .collect();

        assert_eq!(manifest.device_id, "test-device");
        assert_eq!(manifest.generated_at, 123);
        assert_eq!(
            ordered_paths,
            vec!["commands/a.md", "rules/a.rules", "rules/z.rules"]
        );
        assert!(manifest.items[1].is_device_specific);
        assert_eq!(manifest.items[2].content_hash, items[0].content_hash);
    }

    #[test]
    fn diff_manifests_reports_local_remote_modified_and_unchanged() {
        let local = SyncManifest::from_items(
            "local".to_string(),
            100,
            &[
                ConfigItem::new(
                    Tool::Codex,
                    Category::Settings,
                    "config.toml".to_string(),
                    "same".to_string(),
                    1,
                    false,
                ),
                ConfigItem::new(
                    Tool::Codex,
                    Category::Rules,
                    "rules/local.rules".to_string(),
                    "local-only".to_string(),
                    2,
                    false,
                ),
                ConfigItem::new(
                    Tool::Cursor,
                    Category::Commands,
                    "commands/shared.md".to_string(),
                    "local-version".to_string(),
                    3,
                    false,
                ),
            ],
        );
        let remote = SyncManifest::from_items(
            "remote".to_string(),
            200,
            &[
                ConfigItem::new(
                    Tool::Codex,
                    Category::Settings,
                    "config.toml".to_string(),
                    "same".to_string(),
                    5,
                    false,
                ),
                ConfigItem::new(
                    Tool::Cursor,
                    Category::Commands,
                    "commands/shared.md".to_string(),
                    "remote-version".to_string(),
                    6,
                    false,
                ),
                ConfigItem::new(
                    Tool::SharedAgents,
                    Category::Skills,
                    "skills/remote/SKILL.md".to_string(),
                    "remote-only".to_string(),
                    7,
                    false,
                ),
            ],
        );

        let diff = diff_manifests(&local, &remote);
        let summary = summarize_manifest_diff(&diff);
        let statuses: Vec<_> = diff
            .iter()
            .map(|entry| (entry.rel_path.as_str(), entry.status))
            .collect();

        assert_eq!(
            statuses,
            vec![
                ("rules/local.rules", DiffStatus::LocalOnly),
                ("config.toml", DiffStatus::Unchanged),
                ("commands/shared.md", DiffStatus::Modified),
                ("skills/remote/SKILL.md", DiffStatus::RemoteOnly),
            ]
        );
        assert_eq!(
            summary,
            ManifestDiffSummary {
                local_only: 1,
                remote_only: 1,
                modified: 1,
                conflict: 0,
                unchanged: 1,
            }
        );
        assert!(diff[0].local.is_some());
        assert!(diff[0].remote.is_none());
        assert!(diff[1].local.is_some());
        assert!(diff[1].remote.is_some());
    }
}
