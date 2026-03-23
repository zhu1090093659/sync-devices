use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Terminal;
use similar::{ChangeTag, TextDiff};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::{self, Stdout};
use std::time::Duration;

use crate::adapter;
use crate::model::{Category, ConfigItem, DiffStatus, Tool};
use crate::transport::{self, ConfigListFilters, ConfigUploadRequest, RemoteConfigRecord};

const TICK_RATE_MS: u64 = 200;

type AppTerminal = Terminal<CrosstermBackend<Stdout>>;

// -- Display helpers ----------------------------------------------------------

fn tool_label(tool: Tool) -> &'static str {
    match tool {
        Tool::ClaudeCode => "Claude Code",
        Tool::Codex => "Codex",
        Tool::Cursor => "Cursor",
        Tool::SharedAgents => "Shared Agents",
    }
}

fn category_label(cat: Category) -> &'static str {
    match cat {
        Category::Settings => "Settings",
        Category::Instructions => "Instructions",
        Category::Commands => "Commands",
        Category::Skills => "Skills",
        Category::Mcp => "MCP",
        Category::Plugins => "Plugins",
        Category::Rules => "Rules",
    }
}

fn diff_indicator(status: Option<DiffStatus>) -> (&'static str, Color) {
    match status {
        Some(DiffStatus::LocalOnly) => ("+", Color::Green),
        Some(DiffStatus::RemoteOnly) => ("R", Color::Blue),
        Some(DiffStatus::Modified) => ("~", Color::Yellow),
        Some(DiffStatus::Conflict) => ("!", Color::Red),
        Some(DiffStatus::Unchanged) => ("=", Color::DarkGray),
        None => (" ", Color::DarkGray),
    }
}

fn diff_status_label(status: DiffStatus) -> &'static str {
    match status {
        DiffStatus::LocalOnly => "Local Only",
        DiffStatus::RemoteOnly => "Remote Only",
        DiffStatus::Modified => "Modified",
        DiffStatus::Conflict => "Conflict",
        DiffStatus::Unchanged => "Unchanged",
    }
}

// -- Data types ---------------------------------------------------------------

type ItemKey = (String, String, String);

/// Remote data fetched from the API.
struct RemoteData {
    records: BTreeMap<ItemKey, RemoteConfigRecord>,
}

/// A config item carrying local/remote content, diff status, and selection state.
#[derive(Debug, Clone)]
struct TreeItem {
    tool: Tool,
    category: Category,
    rel_path: String,
    local_content: Option<String>,
    local_hash: Option<String>,
    local_modified: u64,
    remote_content: Option<String>,
    diff_status: Option<DiffStatus>,
    is_device_specific: bool,
    checked: bool,
}

/// A line in the unified diff view.
#[derive(Debug, Clone)]
enum DiffLine {
    Header(String),
    Context(String),
    Added(String),
    Removed(String),
}

// -- View modes ---------------------------------------------------------------

enum ViewMode {
    Browse,
    Diff(DiffViewState),
    Resolve(ResolveState),
    Devices(DevicesState),
}

struct DiffViewState {
    title: String,
    lines: Vec<DiffLine>,
    scroll_offset: usize,
}

struct ResolveState {
    item_index: usize,
    lines: Vec<DiffLine>,
    scroll_offset: usize,
}

struct DevicesState {
    lines: Vec<String>,
    scroll_offset: usize,
}

// -- Tree row model -----------------------------------------------------------

#[derive(Debug, Clone)]
enum Row {
    Tool {
        tool: Tool,
        count: usize,
        expanded: bool,
    },
    Category {
        tool: Tool,
        category: Category,
        count: usize,
        expanded: bool,
    },
    Item {
        index: usize,
    },
}

// -- App state ----------------------------------------------------------------

struct App {
    should_quit: bool,
    view: ViewMode,

    /// All tree items (flat, indexed by position).
    items: Vec<TreeItem>,
    /// Tree index: tool -> category -> item indices.
    tree: BTreeMap<Tool, BTreeMap<Category, Vec<usize>>>,

    expanded_tools: HashSet<Tool>,
    expanded_cats: HashSet<(Tool, Category)>,
    rows: Vec<Row>,
    selected: usize,
    scroll_offset: usize,

    total_items: usize,
    remote_available: bool,
    device_id: String,
    /// Cloudflare account name from stored session.
    account_name: Option<String>,
    /// Unique device IDs seen in remote records.
    known_devices: Vec<String>,

    /// Transient status message shown in header.
    status_msg: Option<String>,
}

impl App {
    fn new(
        items: Vec<TreeItem>,
        remote_available: bool,
        device_id: String,
        account_name: Option<String>,
        known_devices: Vec<String>,
    ) -> Self {
        let total_items = items.len();
        let tree = build_tree_index(&items);
        let expanded_tools: HashSet<Tool> = tree.keys().copied().collect();

        let mut app = Self {
            should_quit: false,
            view: ViewMode::Browse,
            items,
            tree,
            expanded_tools,
            expanded_cats: HashSet::new(),
            rows: Vec::new(),
            selected: 0,
            scroll_offset: 0,
            total_items,
            remote_available,
            device_id,
            account_name,
            known_devices,
            status_msg: None,
        };
        app.rebuild_rows();
        app
    }

    fn rebuild_rows(&mut self) {
        self.rows.clear();
        for (tool, categories) in &self.tree {
            let tool_count: usize = categories.values().map(|v| v.len()).sum();
            let tool_expanded = self.expanded_tools.contains(tool);

            self.rows.push(Row::Tool {
                tool: *tool,
                count: tool_count,
                expanded: tool_expanded,
            });

            if !tool_expanded {
                continue;
            }

            for (category, indices) in categories {
                let cat_expanded = self.expanded_cats.contains(&(*tool, *category));

                self.rows.push(Row::Category {
                    tool: *tool,
                    category: *category,
                    count: indices.len(),
                    expanded: cat_expanded,
                });

                if !cat_expanded {
                    continue;
                }

                for &idx in indices {
                    self.rows.push(Row::Item { index: idx });
                }
            }
        }

        if !self.rows.is_empty() && self.selected >= self.rows.len() {
            self.selected = self.rows.len() - 1;
        }
    }

    // -- Expand / collapse (Enter/Right) --------------------------------------

    fn expand_or_diff(&mut self) {
        if self.rows.is_empty() {
            return;
        }

        match &self.rows[self.selected] {
            Row::Tool { tool, expanded, .. } => {
                let tool = *tool;
                if *expanded {
                    self.expanded_tools.remove(&tool);
                    self.expanded_cats.retain(|(t, _)| *t != tool);
                } else {
                    self.expanded_tools.insert(tool);
                }
                self.rebuild_rows();
            }
            Row::Category {
                tool,
                category,
                expanded,
                ..
            } => {
                let key = (*tool, *category);
                if *expanded {
                    self.expanded_cats.remove(&key);
                } else {
                    self.expanded_cats.insert(key);
                }
                self.rebuild_rows();
            }
            Row::Item { index } => {
                self.open_diff(*index);
            }
        }
    }

    fn collapse_or_parent(&mut self) {
        if self.rows.is_empty() {
            return;
        }

        match &self.rows[self.selected] {
            Row::Tool { tool, expanded, .. } => {
                if *expanded {
                    let tool = *tool;
                    self.expanded_tools.remove(&tool);
                    self.expanded_cats.retain(|(t, _)| *t != tool);
                    self.rebuild_rows();
                }
            }
            Row::Category {
                tool,
                category,
                expanded,
                ..
            } => {
                if *expanded {
                    self.expanded_cats.remove(&(*tool, *category));
                    self.rebuild_rows();
                } else {
                    let target = *tool;
                    self.jump_back(|r| matches!(r, Row::Tool { tool, .. } if *tool == target));
                }
            }
            Row::Item { .. } => {
                self.jump_to_ancestor();
            }
        }
    }

    // -- Checkbox toggling (Space) --------------------------------------------

    fn toggle_check(&mut self) {
        if self.rows.is_empty() {
            return;
        }

        match &self.rows[self.selected] {
            Row::Item { index } => {
                let idx = *index;
                self.items[idx].checked = !self.items[idx].checked;
            }
            Row::Tool { tool, .. } => {
                let tool = *tool;
                let indices: Vec<usize> = self
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| item.tool == tool)
                    .map(|(i, _)| i)
                    .collect();
                let all_checked = indices.iter().all(|&i| self.items[i].checked);
                for i in indices {
                    self.items[i].checked = !all_checked;
                }
            }
            Row::Category { tool, category, .. } => {
                let tool = *tool;
                let category = *category;
                let indices: Vec<usize> = self
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| item.tool == tool && item.category == category)
                    .map(|(i, _)| i)
                    .collect();
                let all_checked = indices.iter().all(|&i| self.items[i].checked);
                for i in indices {
                    self.items[i].checked = !all_checked;
                }
            }
        }
    }

    fn toggle_all(&mut self) {
        let all_checked = self.items.iter().all(|item| item.checked);
        for item in &mut self.items {
            item.checked = !all_checked;
        }
    }

    fn checked_count(&self) -> usize {
        self.items.iter().filter(|item| item.checked).count()
    }

    // -- Diff view ------------------------------------------------------------

    fn open_diff(&mut self, index: usize) {
        let item = &self.items[index];
        let title = format!(
            "{} / {} / {}",
            tool_label(item.tool),
            category_label(item.category),
            item.rel_path
        );
        let lines = build_diff_lines(item);
        self.view = ViewMode::Diff(DiffViewState {
            title,
            lines,
            scroll_offset: 0,
        });
    }

    fn close_diff(&mut self) {
        self.view = ViewMode::Browse;
    }

    // -- Conflict resolution --------------------------------------------------

    fn open_resolve(&mut self, index: usize) {
        let item = &self.items[index];
        if item.diff_status != Some(DiffStatus::Conflict) {
            return;
        }
        let lines = build_diff_lines(item);
        self.view = ViewMode::Resolve(ResolveState {
            item_index: index,
            lines,
            scroll_offset: 0,
        });
    }

    fn open_devices(&mut self) {
        let mut lines = Vec::new();
        lines.push(format!("Current Device: {}", self.device_id));
        if let Some(name) = &self.account_name {
            lines.push(format!("Account: {}", name));
        }
        lines.push(format!(
            "Remote: {}",
            if self.remote_available {
                "connected"
            } else {
                "offline"
            }
        ));
        lines.push(String::new());
        lines.push(format!("Known Devices ({}):", self.known_devices.len()));
        for device in &self.known_devices {
            let marker = if *device == self.device_id {
                " (this device)"
            } else {
                ""
            };
            lines.push(format!("  - {}{}", device, marker));
        }
        if self.known_devices.is_empty() {
            lines.push("  (none)".to_string());
        }
        self.view = ViewMode::Devices(DevicesState {
            lines,
            scroll_offset: 0,
        });
    }

    // -- Navigation -----------------------------------------------------------

    fn move_up(&mut self) {
        match &mut self.view {
            ViewMode::Browse => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
            }
            ViewMode::Diff(state) => {
                state.scroll_offset = state.scroll_offset.saturating_sub(1);
            }
            ViewMode::Resolve(state) => {
                state.scroll_offset = state.scroll_offset.saturating_sub(1);
            }
            ViewMode::Devices(state) => {
                state.scroll_offset = state.scroll_offset.saturating_sub(1);
            }
        }
    }

    fn move_down(&mut self) {
        match &mut self.view {
            ViewMode::Browse => {
                if !self.rows.is_empty() && self.selected < self.rows.len() - 1 {
                    self.selected += 1;
                }
            }
            ViewMode::Diff(state) => {
                if state.scroll_offset + 1 < state.lines.len() {
                    state.scroll_offset += 1;
                }
            }
            ViewMode::Resolve(state) => {
                if state.scroll_offset + 1 < state.lines.len() {
                    state.scroll_offset += 1;
                }
            }
            ViewMode::Devices(state) => {
                if state.scroll_offset + 1 < state.lines.len() {
                    state.scroll_offset += 1;
                }
            }
        }
    }

    fn page_up(&mut self, page_size: usize) {
        match &mut self.view {
            ViewMode::Browse => {
                self.selected = self.selected.saturating_sub(page_size);
            }
            ViewMode::Diff(state) => {
                state.scroll_offset = state.scroll_offset.saturating_sub(page_size);
            }
            ViewMode::Resolve(state) => {
                state.scroll_offset = state.scroll_offset.saturating_sub(page_size);
            }
            ViewMode::Devices(state) => {
                state.scroll_offset = state.scroll_offset.saturating_sub(page_size);
            }
        }
    }

    fn page_down(&mut self, page_size: usize) {
        match &mut self.view {
            ViewMode::Browse => {
                if !self.rows.is_empty() {
                    self.selected = (self.selected + page_size).min(self.rows.len() - 1);
                }
            }
            ViewMode::Diff(state) => {
                if !state.lines.is_empty() {
                    state.scroll_offset =
                        (state.scroll_offset + page_size).min(state.lines.len() - 1);
                }
            }
            ViewMode::Resolve(state) => {
                if !state.lines.is_empty() {
                    state.scroll_offset =
                        (state.scroll_offset + page_size).min(state.lines.len() - 1);
                }
            }
            ViewMode::Devices(state) => {
                if !state.lines.is_empty() {
                    state.scroll_offset =
                        (state.scroll_offset + page_size).min(state.lines.len() - 1);
                }
            }
        }
    }

    fn ensure_visible(&mut self, viewport_height: usize) {
        if viewport_height == 0 {
            return;
        }
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + viewport_height {
            self.scroll_offset = self.selected - viewport_height + 1;
        }
    }

    fn jump_back(&mut self, predicate: impl Fn(&Row) -> bool) {
        for i in (0..self.selected).rev() {
            if predicate(&self.rows[i]) {
                self.selected = i;
                return;
            }
        }
    }

    fn jump_to_ancestor(&mut self) {
        for i in (0..self.selected).rev() {
            if matches!(&self.rows[i], Row::Tool { .. } | Row::Category { .. }) {
                self.selected = i;
                return;
            }
        }
    }
}

// -- Tree construction --------------------------------------------------------

fn build_tree_index(items: &[TreeItem]) -> BTreeMap<Tool, BTreeMap<Category, Vec<usize>>> {
    let mut tree: BTreeMap<Tool, BTreeMap<Category, Vec<usize>>> = BTreeMap::new();
    for (i, item) in items.iter().enumerate() {
        tree.entry(item.tool)
            .or_default()
            .entry(item.category)
            .or_default()
            .push(i);
    }
    tree
}

fn build_tree_items(
    snapshot: &adapter::LocalSnapshot,
    remote: Option<&RemoteData>,
) -> Vec<TreeItem> {
    let local_map: BTreeMap<ItemKey, &ConfigItem> = snapshot
        .items
        .iter()
        .map(|item| {
            (
                (
                    item.tool.as_str().to_string(),
                    item.category.as_str().to_string(),
                    item.rel_path.clone(),
                ),
                item,
            )
        })
        .collect();

    let mut result = Vec::new();

    match remote {
        None => {
            for item in &snapshot.items {
                result.push(TreeItem {
                    tool: item.tool,
                    category: item.category,
                    rel_path: item.rel_path.clone(),
                    local_content: Some(item.content.clone()),
                    local_hash: Some(item.content_hash.clone()),
                    local_modified: item.last_modified,
                    remote_content: None,
                    diff_status: None,
                    is_device_specific: item.is_device_specific,
                    checked: false,
                });
            }
        }
        Some(remote_data) => {
            let all_keys: BTreeSet<&ItemKey> =
                local_map.keys().chain(remote_data.records.keys()).collect();

            for key in all_keys {
                let local = local_map.get(key);
                let remote = remote_data.records.get(key);

                let (tool_str, cat_str, rel_path) = key;
                let tool = match Tool::parse(tool_str) {
                    Some(t) => t,
                    None => continue,
                };
                let category = match Category::parse(cat_str) {
                    Some(c) => c,
                    None => continue,
                };

                let diff_status = match (local, remote) {
                    (Some(_), None) => DiffStatus::LocalOnly,
                    (None, Some(_)) => DiffStatus::RemoteOnly,
                    (Some(l), Some(r)) => {
                        if l.content_hash == r.content_hash {
                            DiffStatus::Unchanged
                        } else if !r.device_id.is_empty()
                            && r.device_id != snapshot.manifest.device_id
                        {
                            DiffStatus::Conflict
                        } else {
                            DiffStatus::Modified
                        }
                    }
                    (None, None) => continue,
                };

                let is_device_specific = local
                    .map(|l| l.is_device_specific)
                    .or(remote.map(|r| r.is_device_specific))
                    .unwrap_or(false);

                result.push(TreeItem {
                    tool,
                    category,
                    rel_path: rel_path.clone(),
                    local_content: local.map(|l| l.content.clone()),
                    local_hash: local.map(|l| l.content_hash.clone()),
                    local_modified: local.map(|l| l.last_modified).unwrap_or(0),
                    remote_content: remote.map(|r| r.content.clone()),
                    diff_status: Some(diff_status),
                    is_device_specific,
                    checked: false,
                });
            }
        }
    }

    result.sort_by(|a, b| {
        a.tool
            .cmp(&b.tool)
            .then(a.category.cmp(&b.category))
            .then(a.rel_path.cmp(&b.rel_path))
    });

    result
}

// -- Diff computation ---------------------------------------------------------

fn build_diff_lines(item: &TreeItem) -> Vec<DiffLine> {
    let local = item.local_content.as_deref().unwrap_or("");
    let remote = item.remote_content.as_deref().unwrap_or("");

    let mut lines = Vec::new();

    if let Some(status) = item.diff_status {
        lines.push(DiffLine::Header(format!(
            "Status: {}",
            diff_status_label(status)
        )));
    }

    if local.is_empty() && remote.is_empty() {
        lines.push(DiffLine::Header("(both sides empty)".to_string()));
        return lines;
    }

    if item.diff_status == Some(DiffStatus::Unchanged) {
        lines.push(DiffLine::Header("(no differences)".to_string()));
        lines.push(DiffLine::Header(String::new()));
        for line in local.lines() {
            lines.push(DiffLine::Context(line.to_string()));
        }
        return lines;
    }

    lines.push(DiffLine::Header("--- Local".to_string()));
    lines.push(DiffLine::Header("+++ Remote".to_string()));
    lines.push(DiffLine::Header(String::new()));

    let diff = TextDiff::from_lines(local, remote);
    for change in diff.iter_all_changes() {
        let content = change.value().trim_end_matches('\n').to_string();
        match change.tag() {
            ChangeTag::Equal => lines.push(DiffLine::Context(content)),
            ChangeTag::Delete => lines.push(DiffLine::Removed(content)),
            ChangeTag::Insert => lines.push(DiffLine::Added(content)),
        }
    }

    lines
}

// -- Push / Pull execution ----------------------------------------------------

/// Push checked LocalOnly/Modified items to the remote.
fn execute_push(
    items: &mut [TreeItem],
    client: &transport::ApiTransport,
    device_id: &str,
) -> String {
    // Collect payloads before entering the async block.
    #[allow(clippy::type_complexity)]
    let candidates: Vec<(
        usize,
        String,
        String,
        String,
        String,
        Option<String>,
        u64,
        bool,
    )> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            item.checked
                && item.local_content.is_some()
                && matches!(
                    item.diff_status,
                    Some(DiffStatus::LocalOnly | DiffStatus::Modified | DiffStatus::Conflict)
                )
        })
        .map(|(i, item)| {
            (
                i,
                item.tool.as_str().to_string(),
                item.category.as_str().to_string(),
                item.rel_path.clone(),
                item.local_content.clone().unwrap(),
                item.local_hash.clone(),
                item.local_modified,
                item.is_device_specific,
            )
        })
        .collect();

    if candidates.is_empty() {
        return "No pushable items selected.".to_string();
    }

    let count = candidates.len();
    let handle = tokio::runtime::Handle::current();
    let pushed = tokio::task::block_in_place(|| {
        handle.block_on(async {
            let mut ok = 0usize;
            for &(_, ref tool, ref cat, ref rel, ref content, ref hash, modified, dev_specific) in
                &candidates
            {
                if client
                    .upload_config(
                        tool,
                        cat,
                        rel,
                        &ConfigUploadRequest {
                            content: content.clone(),
                            content_hash: hash.clone(),
                            last_modified: modified,
                            device_id: Some(device_id.to_string()),
                            is_device_specific: Some(dev_specific),
                        },
                    )
                    .await
                    .is_ok()
                {
                    ok += 1;
                }
            }
            ok
        })
    });

    // Mark successfully-pushed items as synced.
    for (idx, ..) in &candidates {
        items[*idx].diff_status = Some(DiffStatus::Unchanged);
        items[*idx].remote_content = items[*idx].local_content.clone();
        items[*idx].checked = false;
    }

    format!("Pushed {}/{} items.", pushed, count)
}

/// Pull checked RemoteOnly items to local disk.
fn execute_pull(items: &mut [TreeItem]) -> String {
    let candidates: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            item.checked
                && item.remote_content.is_some()
                && matches!(
                    item.diff_status,
                    Some(DiffStatus::RemoteOnly | DiffStatus::Conflict)
                )
        })
        .map(|(i, _)| i)
        .collect();

    if candidates.is_empty() {
        return "No pullable items selected.".to_string();
    }

    let mut applied = 0usize;
    let count = candidates.len();

    for &idx in &candidates {
        let item = &items[idx];
        let content = item.remote_content.as_deref().unwrap();
        let path = match adapter::resolve_local_path(item.tool, &item.rel_path) {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Create parent directories.
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        if std::fs::write(&path, content).is_ok() {
            applied += 1;
        }
    }

    // Mark pulled items as synced.
    for &idx in &candidates {
        items[idx].diff_status = Some(DiffStatus::Unchanged);
        items[idx].local_content = items[idx].remote_content.clone();
        items[idx].checked = false;
    }

    format!("Pulled {}/{} items.", applied, count)
}

// -- Conflict resolution helpers ----------------------------------------------

/// Resolve a conflict by pushing the local version to the remote.
fn resolve_keep_local(
    item: &mut TreeItem,
    transport: Option<&transport::ApiTransport>,
    device_id: &str,
) -> String {
    let client = match transport {
        Some(c) => c,
        None => return "Not connected to remote.".to_string(),
    };
    let content = match &item.local_content {
        Some(c) => c.clone(),
        None => return "No local content to push.".to_string(),
    };

    let tool = item.tool.as_str().to_string();
    let category = item.category.as_str().to_string();
    let rel_path = item.rel_path.clone();
    let hash = item.local_hash.clone();
    let modified = item.local_modified;
    let dev_id = device_id.to_string();
    let dev_specific = item.is_device_specific;

    let handle = tokio::runtime::Handle::current();
    let ok = tokio::task::block_in_place(|| {
        handle.block_on(async {
            client
                .upload_config(
                    &tool,
                    &category,
                    &rel_path,
                    &ConfigUploadRequest {
                        content: content.clone(),
                        content_hash: hash,
                        last_modified: modified,
                        device_id: Some(dev_id),
                        is_device_specific: Some(dev_specific),
                    },
                )
                .await
                .is_ok()
        })
    });

    if ok {
        item.diff_status = Some(DiffStatus::Unchanged);
        item.remote_content = Some(content);
        item.checked = false;
        "Conflict resolved: kept local.".to_string()
    } else {
        "Failed to push local version.".to_string()
    }
}

/// Resolve a conflict by writing the remote version to local disk.
fn resolve_keep_remote(item: &mut TreeItem) -> String {
    let content = match &item.remote_content {
        Some(c) => c.clone(),
        None => return "No remote content to pull.".to_string(),
    };

    let path = match adapter::resolve_local_path(item.tool, &item.rel_path) {
        Ok(p) => p,
        Err(_) => return "Cannot resolve local path.".to_string(),
    };

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    if std::fs::write(&path, &content).is_ok() {
        item.diff_status = Some(DiffStatus::Unchanged);
        item.local_content = Some(content);
        item.checked = false;
        "Conflict resolved: kept remote.".to_string()
    } else {
        "Failed to write remote version.".to_string()
    }
}

/// Collect unique device IDs from remote records.
fn collect_known_devices(remote: Option<&RemoteData>) -> Vec<String> {
    let Some(data) = remote else {
        return Vec::new();
    };
    let mut devices: BTreeSet<String> = BTreeSet::new();
    for record in data.records.values() {
        if !record.device_id.is_empty() {
            devices.insert(record.device_id.clone());
        }
    }
    devices.into_iter().collect()
}

// -- Remote data loading ------------------------------------------------------

async fn load_remote_data() -> (Option<RemoteData>, Option<transport::ApiTransport>) {
    let client = match transport::ApiTransport::from_session_store() {
        Ok(c) => c,
        Err(_) => return (None, None),
    };

    let records_list = match client.list_configs(ConfigListFilters::default()).await {
        Ok(r) => r,
        Err(_) => return (None, Some(client)),
    };

    let records = records_list
        .into_iter()
        .map(|r| ((r.tool.clone(), r.category.clone(), r.rel_path.clone()), r))
        .collect();

    (Some(RemoteData { records }), Some(client))
}

// -- Entry point --------------------------------------------------------------

pub async fn run_manage() -> Result<()> {
    let snapshot = adapter::scan_local_snapshot()?;
    let device_id = snapshot.manifest.device_id.clone();
    let (remote, transport) = load_remote_data().await;
    let remote_available = remote.is_some();
    let tree_items = build_tree_items(&snapshot, remote.as_ref());

    // Collect known device IDs from remote records.
    let known_devices = collect_known_devices(remote.as_ref());
    // Load account name from stored session.
    let account_name = crate::session_store::SessionStore::new()
        .ok()
        .and_then(|store| store.load().ok().flatten())
        .map(|s| s.account_name);

    let mut terminal = setup_terminal()?;
    let result = run_app(
        &mut terminal,
        tree_items,
        remote_available,
        device_id,
        account_name,
        known_devices,
        transport,
    );
    let restore_result = restore_terminal(&mut terminal);

    match (result, restore_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Err(error), Err(_)) => Err(error),
    }
}

fn setup_terminal() -> Result<AppTerminal> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut AppTerminal) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

// -- Event loop ---------------------------------------------------------------

fn run_app(
    terminal: &mut AppTerminal,
    items: Vec<TreeItem>,
    remote_available: bool,
    device_id: String,
    account_name: Option<String>,
    known_devices: Vec<String>,
    transport: Option<transport::ApiTransport>,
) -> Result<()> {
    let mut app = App::new(
        items,
        remote_available,
        device_id,
        account_name,
        known_devices,
    );
    let mut page_size: usize = 20;

    while !app.should_quit {
        terminal.draw(|frame| {
            page_size = frame.area().height.saturating_sub(8) as usize;
            render(frame, &mut app);
        })?;

        if !event::poll(Duration::from_millis(TICK_RATE_MS))? {
            continue;
        }

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            // Clear transient status on any keypress.
            app.status_msg = None;

            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                app.should_quit = true;
                continue;
            }

            match &app.view {
                ViewMode::Browse => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
                    KeyCode::Up | KeyCode::Char('k') => app.move_up(),
                    KeyCode::Down | KeyCode::Char('j') => app.move_down(),
                    KeyCode::PageUp => app.page_up(page_size),
                    KeyCode::PageDown => app.page_down(page_size),
                    KeyCode::Enter | KeyCode::Right => app.expand_or_diff(),
                    KeyCode::Char(' ') => app.toggle_check(),
                    KeyCode::Left => app.collapse_or_parent(),
                    KeyCode::Char('a') => app.toggle_all(),
                    KeyCode::Char('d') => {
                        if let Some(Row::Item { index }) = app.rows.get(app.selected) {
                            app.open_diff(*index);
                        }
                    }
                    KeyCode::Char('p') => {
                        if let Some(ref client) = transport {
                            let msg = execute_push(&mut app.items, client, &app.device_id);
                            app.status_msg = Some(msg);
                        } else {
                            app.status_msg = Some("Not connected to remote.".to_string());
                        }
                    }
                    KeyCode::Char('l') => {
                        if app.remote_available {
                            let msg = execute_pull(&mut app.items);
                            app.status_msg = Some(msg);
                        } else {
                            app.status_msg = Some("Not connected to remote.".to_string());
                        }
                    }
                    KeyCode::Char('r') => {
                        if let Some(Row::Item { index }) = app.rows.get(app.selected) {
                            app.open_resolve(*index);
                        }
                    }
                    KeyCode::Char('i') => app.open_devices(),
                    _ => {}
                },
                ViewMode::Diff(_) => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => app.close_diff(),
                    KeyCode::Up | KeyCode::Char('k') => app.move_up(),
                    KeyCode::Down | KeyCode::Char('j') => app.move_down(),
                    KeyCode::PageUp => app.page_up(page_size),
                    KeyCode::PageDown => app.page_down(page_size),
                    _ => {}
                },
                ViewMode::Resolve(_) => {
                    // Extract item index before consuming the match borrow.
                    let idx = match &app.view {
                        ViewMode::Resolve(s) => s.item_index,
                        _ => unreachable!(),
                    };
                    match key.code {
                        KeyCode::Char('1') => {
                            // Keep local → push to remote.
                            let msg = resolve_keep_local(
                                &mut app.items[idx],
                                transport.as_ref(),
                                &app.device_id,
                            );
                            app.status_msg = Some(msg);
                            app.view = ViewMode::Browse;
                        }
                        KeyCode::Char('2') => {
                            // Keep remote → write to local disk.
                            let msg = resolve_keep_remote(&mut app.items[idx]);
                            app.status_msg = Some(msg);
                            app.view = ViewMode::Browse;
                        }
                        KeyCode::Char('q') | KeyCode::Esc => {
                            app.view = ViewMode::Browse;
                        }
                        KeyCode::Up | KeyCode::Char('k') => app.move_up(),
                        KeyCode::Down | KeyCode::Char('j') => app.move_down(),
                        KeyCode::PageUp => app.page_up(page_size),
                        KeyCode::PageDown => app.page_down(page_size),
                        _ => {}
                    }
                }
                ViewMode::Devices(_) => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        app.view = ViewMode::Browse;
                    }
                    KeyCode::Up | KeyCode::Char('k') => app.move_up(),
                    KeyCode::Down | KeyCode::Char('j') => app.move_down(),
                    KeyCode::PageUp => app.page_up(page_size),
                    KeyCode::PageDown => app.page_down(page_size),
                    _ => {}
                },
            }
        }
    }

    Ok(())
}

// -- Rendering ----------------------------------------------------------------

fn render(frame: &mut ratatui::Frame, app: &mut App) {
    match &app.view {
        ViewMode::Browse => render_browse(frame, app),
        ViewMode::Diff(_) => render_diff(frame, app),
        ViewMode::Resolve(_) => render_resolve(frame, app),
        ViewMode::Devices(_) => render_devices(frame, app),
    }
}

fn render_browse(frame: &mut ratatui::Frame, app: &mut App) {
    let area = frame.area();
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(3),
        ])
        .split(area);

    // Header
    let status = if app.remote_available {
        "online"
    } else {
        "offline"
    };
    let checked = app.checked_count();
    let header_text = match &app.status_msg {
        Some(msg) => format!(
            "{} items | {} | {} selected | {}",
            app.total_items, status, checked, msg
        ),
        None => format!(
            "sync-devices manage | {} items | {} | {} selected",
            app.total_items, status, checked
        ),
    };
    let header = Paragraph::new(Line::from(header_text))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Config Browser"),
        )
        .style(Style::default().add_modifier(Modifier::BOLD));
    frame.render_widget(header, sections[0]);

    // Body: tree view
    let body_block = Block::default()
        .borders(Borders::ALL)
        .title("Configurations");
    let inner = body_block.inner(sections[1]);
    let vh = inner.height as usize;

    app.ensure_visible(vh);

    let body = if app.rows.is_empty() {
        Paragraph::new(Line::from("  No configuration items found."))
            .style(Style::default().fg(Color::DarkGray))
            .block(body_block)
    } else {
        let lines: Vec<Line> = app
            .rows
            .iter()
            .enumerate()
            .skip(app.scroll_offset)
            .take(vh)
            .map(|(i, row)| render_browse_row(row, &app.items, i == app.selected))
            .collect();
        Paragraph::new(lines).block(body_block)
    };
    frame.render_widget(body, sections[1]);

    // Footer
    let hint = if app.remote_available {
        "Space Check  a All  Enter Expand/Diff  d Diff  p Push  l Pull  q Quit"
    } else {
        "Space Check  a All  Enter Expand/Diff  d Diff  q Quit"
    };
    let footer = Paragraph::new(Line::from(hint))
        .block(Block::default().borders(Borders::ALL).title("Controls"));
    frame.render_widget(footer, sections[2]);
}

fn render_browse_row(row: &Row, items: &[TreeItem], selected: bool) -> Line<'static> {
    let (indent, text, style) = match row {
        Row::Tool {
            tool,
            count,
            expanded,
        } => {
            let arrow = if *expanded { "▼" } else { "▶" };
            let text = format!("{} {} ({})", arrow, tool_label(*tool), count);
            let s = Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD);
            ("", text, s)
        }
        Row::Category {
            category,
            count,
            expanded,
            ..
        } => {
            let arrow = if *expanded { "▼" } else { "▶" };
            let text = format!("{} {} ({})", arrow, category_label(*category), count);
            ("  ", text, Style::default().fg(Color::Yellow))
        }
        Row::Item { index } => {
            let item = &items[*index];
            let check = if item.checked { "x" } else { " " };
            let (tag, color) = diff_indicator(item.diff_status);
            let dev = if item.is_device_specific {
                " [device]"
            } else {
                ""
            };
            let text = format!("[{}] [{}] {}{}", check, tag, item.rel_path, dev);
            ("    ", text, Style::default().fg(color))
        }
    };

    let full = format!("{}{}", indent, text);
    let final_style = if selected {
        style.add_modifier(Modifier::REVERSED)
    } else {
        style
    };
    Line::from(Span::styled(full, final_style))
}

fn render_diff(frame: &mut ratatui::Frame, app: &mut App) {
    let area = frame.area();
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(3),
        ])
        .split(area);

    let (title, diff_lines, scroll) = match &app.view {
        ViewMode::Diff(s) => (&s.title, &s.lines, s.scroll_offset),
        _ => unreachable!(),
    };

    let header = Paragraph::new(Line::from(title.clone()))
        .block(Block::default().borders(Borders::ALL).title("Diff View"))
        .style(Style::default().add_modifier(Modifier::BOLD));
    frame.render_widget(header, sections[0]);

    let body_block = Block::default().borders(Borders::ALL).title("Content Diff");
    let inner = body_block.inner(sections[1]);
    let vh = inner.height as usize;

    let lines: Vec<Line> = diff_lines
        .iter()
        .skip(scroll)
        .take(vh)
        .map(render_diff_line)
        .collect();
    let body = Paragraph::new(lines).block(body_block);
    frame.render_widget(body, sections[1]);

    let footer = Paragraph::new(Line::from("Up/Down Scroll  PgUp/PgDn Page  q/Esc Back"))
        .block(Block::default().borders(Borders::ALL).title("Controls"));
    frame.render_widget(footer, sections[2]);
}

fn render_resolve(frame: &mut ratatui::Frame, app: &mut App) {
    let area = frame.area();
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(3),
        ])
        .split(area);

    let (idx, lines, scroll) = match &app.view {
        ViewMode::Resolve(s) => (s.item_index, &s.lines, s.scroll_offset),
        _ => unreachable!(),
    };

    let item = &app.items[idx];
    let title = format!(
        "CONFLICT: {} / {} / {}",
        tool_label(item.tool),
        category_label(item.category),
        item.rel_path
    );
    let header = Paragraph::new(Line::from(title))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Conflict Resolution"),
        )
        .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
    frame.render_widget(header, sections[0]);

    let body_block = Block::default()
        .borders(Borders::ALL)
        .title("Local vs Remote");
    let inner = body_block.inner(sections[1]);
    let vh = inner.height as usize;

    let rendered: Vec<Line> = lines
        .iter()
        .skip(scroll)
        .take(vh)
        .map(render_diff_line)
        .collect();
    let body = Paragraph::new(rendered).block(body_block);
    frame.render_widget(body, sections[1]);

    let footer = Paragraph::new(Line::from(
        "1 Keep Local  2 Keep Remote  Up/Down Scroll  Esc Cancel",
    ))
    .block(Block::default().borders(Borders::ALL).title("Resolve"));
    frame.render_widget(footer, sections[2]);
}

fn render_devices(frame: &mut ratatui::Frame, app: &mut App) {
    let area = frame.area();
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(3),
        ])
        .split(area);

    let (lines, scroll) = match &app.view {
        ViewMode::Devices(s) => (&s.lines, s.scroll_offset),
        _ => unreachable!(),
    };

    let header = Paragraph::new(Line::from("Device & Session Info"))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Device Management"),
        )
        .style(Style::default().add_modifier(Modifier::BOLD));
    frame.render_widget(header, sections[0]);

    let body_block = Block::default().borders(Borders::ALL).title("Details");
    let inner = body_block.inner(sections[1]);
    let vh = inner.height as usize;

    let rendered: Vec<Line> = lines
        .iter()
        .skip(scroll)
        .take(vh)
        .map(|text| {
            Line::from(Span::styled(
                text.clone(),
                Style::default().fg(Color::White),
            ))
        })
        .collect();
    let body = Paragraph::new(rendered).block(body_block);
    frame.render_widget(body, sections[1]);

    let footer = Paragraph::new(Line::from("Up/Down Scroll  q/Esc Back"))
        .block(Block::default().borders(Borders::ALL).title("Controls"));
    frame.render_widget(footer, sections[2]);
}

fn render_diff_line(line: &DiffLine) -> Line<'static> {
    match line {
        DiffLine::Header(text) => Line::from(Span::styled(
            text.clone(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        DiffLine::Context(text) => Line::from(Span::styled(
            format!("  {}", text),
            Style::default().fg(Color::DarkGray),
        )),
        DiffLine::Removed(text) => Line::from(Span::styled(
            format!("- {}", text),
            Style::default().fg(Color::Red),
        )),
        DiffLine::Added(text) => Line::from(Span::styled(
            format!("+ {}", text),
            Style::default().fg(Color::Green),
        )),
    }
}
