use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Supported AI CLI tools
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    ClaudeCode,
    Codex,
    Cursor,
    SharedAgents,
}

/// Configuration item categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
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

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "claude_code" => Some(Self::ClaudeCode),
            "codex" => Some(Self::Codex),
            "cursor" => Some(Self::Cursor),
            "shared_agents" => Some(Self::SharedAgents),
            _ => None,
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

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "settings" => Some(Self::Settings),
            "instructions" => Some(Self::Instructions),
            "commands" => Some(Self::Commands),
            "skills" => Some(Self::Skills),
            "mcp" => Some(Self::Mcp),
            "plugins" => Some(Self::Plugins),
            "rules" => Some(Self::Rules),
            _ => None,
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
    #[serde(default)]
    pub device_id: String,
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
            device_id: item.device_id.clone(),
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

/// A single upload candidate derived from manifest diff results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushPlanItem {
    pub tool: Tool,
    pub category: Category,
    pub rel_path: String,
    pub status: DiffStatus,
}

/// Compare local and remote manifests using tool/category/path identity plus metadata.
pub fn diff_manifests(local: &SyncManifest, remote: &SyncManifest) -> Vec<ManifestDiffEntry> {
    type Key<'a> = (&'a str, &'a str, &'a str);

    let local_map = index_manifest_entries(&local.items);
    let remote_map = index_manifest_entries(&remote.items);
    let keys: BTreeSet<Key> = local_map.keys().chain(remote_map.keys()).copied().collect();

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
            (Some(l), Some(r)) if is_conflict_entry(&local.device_id, l, r) => DiffStatus::Conflict,
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

/// Build an incremental push plan from manifest diff results.
pub fn build_push_plan(entries: &[ManifestDiffEntry]) -> Vec<PushPlanItem> {
    entries
        .iter()
        .filter_map(|entry| match entry.status {
            DiffStatus::LocalOnly | DiffStatus::Modified => Some(PushPlanItem {
                tool: entry.tool,
                category: entry.category,
                rel_path: entry.rel_path.clone(),
                status: entry.status,
            }),
            DiffStatus::RemoteOnly | DiffStatus::Conflict | DiffStatus::Unchanged => None,
        })
        .collect()
}

fn compute_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn index_manifest_entries(items: &[ManifestEntry]) -> BTreeMap<(&str, &str, &str), &ManifestEntry> {
    items
        .iter()
        .map(|item| {
            (
                (
                    item.tool.as_str(),
                    item.category.as_str(),
                    item.rel_path.as_str(),
                ),
                item,
            )
        })
        .collect()
}

fn is_same_manifest_entry(left: &ManifestEntry, right: &ManifestEntry) -> bool {
    left.content_hash == right.content_hash && left.is_device_specific == right.is_device_specific
}

fn is_conflict_entry(local_device_id: &str, _left: &ManifestEntry, right: &ManifestEntry) -> bool {
    !right.device_id.is_empty() && right.device_id != local_device_id
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item_with_device(
        tool: Tool,
        category: Category,
        rel_path: &str,
        content: &str,
        last_modified: u64,
        is_device_specific: bool,
        device_id: &str,
    ) -> ConfigItem {
        let mut item = ConfigItem::new(
            tool,
            category,
            rel_path.to_string(),
            content.to_string(),
            last_modified,
            is_device_specific,
        );
        item.device_id = device_id.to_string();
        item
    }

    fn make_diff_entry(rel_path: &str, status: DiffStatus) -> ManifestDiffEntry {
        ManifestDiffEntry {
            tool: Tool::Codex,
            category: Category::Settings,
            rel_path: rel_path.to_string(),
            local: None,
            remote: None,
            status,
        }
    }

    #[test]
    fn manifest_from_items_sorts_entries_and_preserves_metadata() {
        let items = vec![
            item_with_device(
                Tool::Codex,
                Category::Rules,
                "rules/z.rules",
                "z",
                30,
                false,
                "test-device",
            ),
            item_with_device(
                Tool::ClaudeCode,
                Category::Commands,
                "commands/a.md",
                "a",
                10,
                false,
                "test-device",
            ),
            item_with_device(
                Tool::Codex,
                Category::Rules,
                "rules/a.rules",
                "b",
                20,
                true,
                "test-device",
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
        assert_eq!(manifest.items[0].device_id, "test-device");
    }

    #[test]
    fn diff_manifests_reports_local_remote_modified_and_unchanged() {
        let local = SyncManifest::from_items(
            "local".to_string(),
            100,
            &[
                item_with_device(
                    Tool::Codex,
                    Category::Settings,
                    "config.toml",
                    "same",
                    1,
                    false,
                    "local",
                ),
                item_with_device(
                    Tool::Codex,
                    Category::Rules,
                    "rules/local.rules",
                    "local-only",
                    2,
                    false,
                    "local",
                ),
                item_with_device(
                    Tool::Cursor,
                    Category::Commands,
                    "commands/shared.md",
                    "local-version",
                    3,
                    false,
                    "local",
                ),
            ],
        );
        let remote = SyncManifest::from_items(
            "remote".to_string(),
            200,
            &[
                item_with_device(
                    Tool::Codex,
                    Category::Settings,
                    "config.toml",
                    "same",
                    5,
                    false,
                    "local",
                ),
                item_with_device(
                    Tool::Cursor,
                    Category::Commands,
                    "commands/shared.md",
                    "remote-version",
                    6,
                    false,
                    "local",
                ),
                item_with_device(
                    Tool::SharedAgents,
                    Category::Skills,
                    "skills/remote/SKILL.md",
                    "remote-only",
                    7,
                    false,
                    "remote",
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

    #[test]
    fn build_push_plan_only_keeps_local_only_and_modified_entries() {
        let local = SyncManifest::from_items(
            "local".to_string(),
            100,
            &[
                item_with_device(
                    Tool::Codex,
                    Category::Settings,
                    "config.toml",
                    "same",
                    1,
                    false,
                    "local",
                ),
                item_with_device(
                    Tool::Codex,
                    Category::Rules,
                    "rules/local.rules",
                    "local-only",
                    2,
                    false,
                    "local",
                ),
                item_with_device(
                    Tool::Cursor,
                    Category::Commands,
                    "commands/shared.md",
                    "local-version",
                    3,
                    false,
                    "local",
                ),
            ],
        );
        let remote = SyncManifest::from_items(
            "remote".to_string(),
            200,
            &[
                item_with_device(
                    Tool::Codex,
                    Category::Settings,
                    "config.toml",
                    "same",
                    5,
                    false,
                    "local",
                ),
                item_with_device(
                    Tool::Cursor,
                    Category::Commands,
                    "commands/shared.md",
                    "remote-version",
                    6,
                    false,
                    "local",
                ),
                item_with_device(
                    Tool::SharedAgents,
                    Category::Skills,
                    "skills/remote/SKILL.md",
                    "remote-only",
                    7,
                    false,
                    "remote",
                ),
            ],
        );

        let diff = diff_manifests(&local, &remote);
        let plan = build_push_plan(&diff);

        assert_eq!(
            plan,
            vec![
                PushPlanItem {
                    tool: Tool::Codex,
                    category: Category::Rules,
                    rel_path: "rules/local.rules".to_string(),
                    status: DiffStatus::LocalOnly,
                },
                PushPlanItem {
                    tool: Tool::Cursor,
                    category: Category::Commands,
                    rel_path: "commands/shared.md".to_string(),
                    status: DiffStatus::Modified,
                },
            ]
        );
    }

    #[test]
    fn config_item_new_computes_deterministic_hash() {
        let a = ConfigItem::new(
            Tool::Codex,
            Category::Settings,
            "config.toml".to_string(),
            "hello world".to_string(),
            1,
            false,
        );
        let b = ConfigItem::new(
            Tool::ClaudeCode,
            Category::Commands,
            "other.md".to_string(),
            "hello world".to_string(),
            999,
            true,
        );
        // same content => same hash regardless of other fields
        assert_eq!(a.content_hash, b.content_hash);
        assert!(!a.content_hash.is_empty());
    }

    #[test]
    fn config_item_different_content_different_hash() {
        let a = ConfigItem::new(
            Tool::Codex,
            Category::Settings,
            "a.toml".to_string(),
            "aaa".to_string(),
            1,
            false,
        );
        let b = ConfigItem::new(
            Tool::Codex,
            Category::Settings,
            "a.toml".to_string(),
            "bbb".to_string(),
            1,
            false,
        );
        assert_ne!(a.content_hash, b.content_hash);
    }

    #[test]
    fn diff_manifests_both_empty_returns_empty() {
        let local = SyncManifest {
            device_id: "local".to_string(),
            generated_at: 0,
            items: vec![],
        };
        let remote = SyncManifest {
            device_id: "remote".to_string(),
            generated_at: 0,
            items: vec![],
        };
        assert!(diff_manifests(&local, &remote).is_empty());
    }

    #[test]
    fn diff_manifests_local_only_when_remote_empty() {
        let local = SyncManifest::from_items(
            "local".to_string(),
            100,
            &[item_with_device(
                Tool::Codex,
                Category::Settings,
                "config.toml",
                "data",
                1,
                false,
                "local",
            )],
        );
        let remote = SyncManifest {
            device_id: "remote".to_string(),
            generated_at: 0,
            items: vec![],
        };
        let diff = diff_manifests(&local, &remote);
        assert_eq!(diff.len(), 1);
        assert_eq!(diff[0].status, DiffStatus::LocalOnly);
    }

    #[test]
    fn summarize_manifest_diff_counts_correctly() {
        let entries = vec![
            make_diff_entry("a", DiffStatus::LocalOnly),
            make_diff_entry("b", DiffStatus::LocalOnly),
            make_diff_entry("c", DiffStatus::RemoteOnly),
            make_diff_entry("d", DiffStatus::Conflict),
        ];
        let summary = summarize_manifest_diff(&entries);
        assert_eq!(summary.local_only, 2);
        assert_eq!(summary.remote_only, 1);
        assert_eq!(summary.conflict, 1);
        assert_eq!(summary.modified, 0);
        assert_eq!(summary.unchanged, 0);
    }

    #[test]
    fn build_push_plan_excludes_conflict_and_remote_only() {
        let entries = vec![
            make_diff_entry("local.toml", DiffStatus::LocalOnly),
            make_diff_entry("conflict.toml", DiffStatus::Conflict),
            make_diff_entry("remote.toml", DiffStatus::RemoteOnly),
            make_diff_entry("same.toml", DiffStatus::Unchanged),
        ];
        let plan = build_push_plan(&entries);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].rel_path, "local.toml");
    }

    #[test]
    fn diff_manifests_marks_remote_changes_from_other_device_as_conflict() {
        let local = SyncManifest::from_items(
            "local-device".to_string(),
            100,
            &[item_with_device(
                Tool::Codex,
                Category::Settings,
                "config.toml",
                "local-version",
                10,
                false,
                "local-device",
            )],
        );
        let remote = SyncManifest::from_items(
            "remote".to_string(),
            200,
            &[item_with_device(
                Tool::Codex,
                Category::Settings,
                "config.toml",
                "remote-version",
                11,
                false,
                "other-device",
            )],
        );

        let diff = diff_manifests(&local, &remote);

        assert_eq!(diff.len(), 1);
        assert_eq!(diff[0].status, DiffStatus::Conflict);
    }
}
