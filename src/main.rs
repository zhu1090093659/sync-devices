mod adapter;
mod auth;
mod cloudflare_api;
mod model;
mod sanitizer;
mod session_store;
mod transport;
mod tui;
mod worker_bundle;

use crate::model::{ConfigItem, DiffStatus};
use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Parser)]
#[command(
    name = "sync-devices",
    version,
    about = "Sync AI CLI tool configurations across devices"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Login with a Cloudflare API Token
    Login,
    /// Logout and clear stored credentials
    Logout,
    /// Deploy Worker and KV to your Cloudflare account
    Setup,
    /// Remove Worker and KV from your Cloudflare account
    Teardown,
    /// Push local configurations to the cloud
    Push,
    /// Pull configurations from the cloud
    Pull,
    /// Show sync status (local vs remote diff)
    Status,
    /// Open interactive TUI for managing configurations
    Manage,
    /// Show diff for a specific tool
    Diff {
        /// Tool name: claude-code, codex, cursor
        tool: String,
    },
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct PushSummary {
    uploaded: usize,
    created: usize,
    modified: usize,
    conflicts: usize,
}

#[derive(Debug)]
struct PushResult {
    summary: PushSummary,
    #[allow(dead_code)] // used in ignored live integration test
    uploaded_records: Vec<transport::RemoteConfigRecord>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct PullSummary {
    applied: usize,
    created: usize,
    updated: usize,
    backed_up: usize,
    skipped_modified: usize,
    skipped_conflicts: usize,
}

#[derive(Debug)]
struct PullApplyResult {
    created: bool,
    backup_path: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Login => {
            println!("Paste your Cloudflare API Token (input is hidden):");
            let api_token = read_hidden_line()?;
            if api_token.is_empty() {
                return Err(anyhow!("API token must not be empty."));
            }

            println!("Verifying token...");
            let account = auth::verify_cf_token(&api_token).await?;

            let store = session_store::SessionStore::new()?;
            store.save(&account, &api_token, None)?;

            println!("Login succeeded.");
            println!("Account: {} ({})", account.account_name, account.account_id);
            println!("Token stored securely in the system keyring.");
            println!("Run `sync-devices setup` to deploy your Worker.");
        }
        Commands::Setup => {
            run_setup().await?;
        }
        Commands::Teardown => {
            run_teardown().await?;
        }
        Commands::Logout => {
            let store = session_store::SessionStore::new()?;
            if store.clear()? {
                println!("Stored session cleared from the system keyring.");
            } else {
                println!("No stored session was found.");
            }
        }
        Commands::Push => {
            let local_snapshot = adapter::scan_local_snapshot()?;
            let result = push_local_changes(&local_snapshot).await?;
            let summary = result.summary;

            if summary.uploaded == 0 {
                if summary.conflicts > 0 {
                    println!(
                        "No safe local changes to push. Detected {} conflict item(s).",
                        summary.conflicts
                    );
                } else {
                    println!("No local changes to push.");
                }
                return Ok(());
            }

            println!(
                "Pushed {} item(s) from device {}.",
                summary.uploaded, local_snapshot.manifest.device_id
            );
            println!("  New: {}", summary.created);
            println!("  Modified: {}", summary.modified);
            if summary.conflicts > 0 {
                println!("  Conflicts skipped: {}", summary.conflicts);
            }
        }
        Commands::Pull => {
            let local_snapshot = adapter::scan_local_snapshot()?;
            println!(
                "Found {} local syncable item(s) on device {}. Fetching remote changes...",
                local_snapshot.manifest.items.len(),
                local_snapshot.manifest.device_id
            );
            let summary = match pull_remote_changes(&local_snapshot).await {
                Ok(summary) => summary,
                Err(error) if is_missing_session_error(&error) => {
                    println!("Remote session unavailable: not logged in. Run `sync-devices login` first.");
                    return Ok(());
                }
                Err(error) => return Err(error),
            };

            if summary.applied == 0 {
                if summary.skipped_conflicts > 0 || summary.skipped_modified > 0 {
                    println!(
                        "No safe remote-only changes to pull. Skipped {} conflict item(s) and {} modified item(s).",
                        summary.skipped_conflicts, summary.skipped_modified
                    );
                } else {
                    println!("No remote changes to pull.");
                }
                return Ok(());
            }

            println!("Pull completed.");
            println!("Applied {} item(s).", summary.applied);
            println!("  Created: {}", summary.created);
            println!("  Updated: {}", summary.updated);
            println!("  Backups: {}", summary.backed_up);
            if summary.skipped_conflicts > 0 {
                println!("  Skipped conflicts: {}", summary.skipped_conflicts);
            }
            if summary.skipped_modified > 0 {
                println!("  Skipped modified: {}", summary.skipped_modified);
            }
        }
        Commands::Status => {
            let local_manifest = adapter::scan_local_manifest()?;
            println!(
                "Found {} syncable config items on device {}:\n",
                local_manifest.items.len(),
                local_manifest.device_id
            );
            for item in &local_manifest.items {
                let marker = if item.is_device_specific {
                    " [device-specific]"
                } else {
                    ""
                };
                println!(
                    "  [{:?}] {:?}/{} {}",
                    item.tool, item.category, item.rel_path, marker
                );
            }

            // Show session and Worker deployment info
            let store = session_store::SessionStore::new()?;
            match store.load()? {
                Some(session) => {
                    println!();
                    println!("Account: {} ({})", session.account_name, session.account_id);
                    match &session.worker_url {
                        Some(url) if !url.is_empty() => {
                            println!("Worker URL: {}", url);
                            // Try to fetch Worker self-description
                            if let Ok(resp) = reqwest::get(url).await {
                                if let Ok(info) = resp.json::<serde_json::Value>().await {
                                    if let Some(version) =
                                        info.get("version").and_then(|v| v.as_str())
                                    {
                                        println!("Worker version: {}", version);
                                    }
                                    if let Some(kv) = info.get("kv_bound").and_then(|v| v.as_bool())
                                    {
                                        println!("KV bound: {}", kv);
                                    }
                                }
                            }
                        }
                        _ => {
                            println!("Worker: not deployed. Run `sync-devices setup` to deploy.");
                        }
                    }
                }
                None => {
                    println!();
                    println!("Not logged in. Run `sync-devices login` first.");
                }
            }

            match transport::ApiTransport::from_session_store() {
                Ok(client) => {
                    let remote_manifest = client.get_manifest().await?;
                    let configs = client
                        .list_configs(transport::ConfigListFilters::default())
                        .await?;
                    let diff = model::diff_manifests(&local_manifest, &remote_manifest);
                    let diff_summary = model::summarize_manifest_diff(&diff);
                    println!();
                    println!("Remote config records: {}", configs.len());
                    println!("Manifest diff:");
                    println!("  Local only: {}", diff_summary.local_only);
                    println!("  Remote only: {}", diff_summary.remote_only);
                    println!("  Modified: {}", diff_summary.modified);
                    println!("  Conflict: {}", diff_summary.conflict);
                    println!("  Unchanged: {}", diff_summary.unchanged);
                }
                Err(transport::TransportError::MissingSession)
                | Err(transport::TransportError::MissingWorkerUrl) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Commands::Manage => {
            tui::run_manage().await?;
        }
        Commands::Diff { tool } => {
            println!("Diff for {} not yet implemented", tool);
        }
    }

    Ok(())
}

async fn push_local_changes(local_snapshot: &adapter::LocalSnapshot) -> Result<PushResult> {
    let client = transport::ApiTransport::from_session_store()?;
    client.check_health().await?;
    let remote_manifest = client.get_manifest().await?;
    let diff = model::diff_manifests(&local_snapshot.manifest, &remote_manifest);
    let push_plan = model::build_push_plan(&diff);
    let item_index = index_config_items(&local_snapshot.items);
    let created = push_plan
        .iter()
        .filter(|item| item.status == DiffStatus::LocalOnly)
        .count();
    let modified = push_plan
        .iter()
        .filter(|item| item.status == DiffStatus::Modified)
        .count();
    let conflicts = diff
        .iter()
        .filter(|entry| entry.status == DiffStatus::Conflict)
        .count();
    let mut uploaded_records = Vec::with_capacity(push_plan.len());

    let pb = ProgressBar::new(push_plan.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:30.cyan/dim}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("=> "),
    );

    for item in &push_plan {
        pb.set_message(format!(
            "{}/{}",
            item.tool.as_str(),
            item.rel_path
        ));

        let config = item_index
            .get(&(
                item.tool.as_str().to_string(),
                item.category.as_str().to_string(),
                item.rel_path.clone(),
            ))
            .with_context(|| {
                format!(
                    "missing local config payload for {} / {} / {}",
                    item.tool.as_str(),
                    item.category.as_str(),
                    item.rel_path
                )
            })?;

        let record = client
            .upload_config(
                item.tool.as_str(),
                item.category.as_str(),
                &item.rel_path,
                &transport::ConfigUploadRequest {
                    content: config.content.clone(),
                    content_hash: Some(config.content_hash.clone()),
                    last_modified: config.last_modified,
                    device_id: Some(config.device_id.clone()),
                    is_device_specific: Some(config.is_device_specific),
                },
            )
            .await?;
        uploaded_records.push(record);
        pb.inc(1);
    }

    pb.finish_and_clear();

    Ok(PushResult {
        summary: PushSummary {
            uploaded: push_plan.len(),
            created,
            modified,
            conflicts,
        },
        uploaded_records,
    })
}

async fn pull_remote_changes(local_snapshot: &adapter::LocalSnapshot) -> Result<PullSummary> {
    let client = transport::ApiTransport::from_session_store()?;
    client.check_health().await?;
    let remote_manifest = client.get_manifest().await?;
    let diff = model::diff_manifests(&local_snapshot.manifest, &remote_manifest);
    let remote_only = diff
        .iter()
        .filter(|entry| entry.status == DiffStatus::RemoteOnly)
        .collect::<Vec<_>>();
    let skipped_modified = diff
        .iter()
        .filter(|entry| entry.status == DiffStatus::Modified)
        .count();
    let skipped_conflicts = diff
        .iter()
        .filter(|entry| entry.status == DiffStatus::Conflict)
        .count();

    if remote_only.is_empty() {
        return Ok(PullSummary {
            skipped_modified,
            skipped_conflicts,
            ..PullSummary::default()
        });
    }

    let remote_records = fetch_remote_records(&client, &remote_only).await?;
    let mut summary = PullSummary {
        skipped_modified,
        skipped_conflicts,
        ..PullSummary::default()
    };

    let pb = ProgressBar::new(remote_only.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:30.cyan/dim}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("=> "),
    );

    for entry in remote_only {
        pb.set_message(format!(
            "{}/{}",
            entry.tool.as_str(),
            entry.rel_path
        ));

        let record = remote_records
            .get(&(
                entry.tool.as_str().to_string(),
                entry.category.as_str().to_string(),
                entry.rel_path.clone(),
            ))
            .with_context(|| {
                format!(
                    "missing remote config payload for {} / {} / {}",
                    entry.tool.as_str(),
                    entry.category.as_str(),
                    entry.rel_path
                )
            })?;
        let applied = apply_remote_record(record)?;

        summary.applied += 1;
        if applied.created {
            summary.created += 1;
        } else {
            summary.updated += 1;
        }
        if applied.backup_path.is_some() {
            summary.backed_up += 1;
        }
        pb.inc(1);
    }

    pb.finish_and_clear();

    Ok(summary)
}

async fn fetch_remote_records(
    client: &transport::ApiTransport,
    entries: &[&model::ManifestDiffEntry],
) -> Result<BTreeMap<(String, String, String), transport::RemoteConfigRecord>> {
    let groups = entries
        .iter()
        .map(|entry| {
            (
                entry.tool.as_str().to_string(),
                entry.category.as_str().to_string(),
            )
        })
        .collect::<BTreeSet<_>>();

    let mut records = BTreeMap::new();
    for (tool, category) in groups {
        let items = client
            .list_configs(transport::ConfigListFilters {
                tool: Some(tool.clone()),
                category: Some(category.clone()),
            })
            .await?;

        for record in items {
            records.insert(
                (
                    record.tool.clone(),
                    record.category.clone(),
                    record.rel_path.clone(),
                ),
                record,
            );
        }
    }

    Ok(records)
}

fn apply_remote_record(record: &transport::RemoteConfigRecord) -> Result<PullApplyResult> {
    let tool = model::Tool::parse(&record.tool)
        .with_context(|| format!("unsupported remote tool '{}'", record.tool))?;
    let category = model::Category::parse(&record.category)
        .with_context(|| format!("unsupported remote category '{}'", record.category))?;
    let target_path = adapter::resolve_local_path(tool, &record.rel_path)?;

    apply_remote_record_to_path(record, tool, category, &target_path)
}

fn apply_remote_record_to_path(
    record: &transport::RemoteConfigRecord,
    tool: model::Tool,
    category: model::Category,
    target_path: &Path,
) -> Result<PullApplyResult> {
    let expected_hash = ConfigItem::new(
        tool,
        category,
        record.rel_path.clone(),
        record.content.clone(),
        record.last_modified,
        record.is_device_specific,
    )
    .content_hash;
    if expected_hash != record.content_hash {
        return Err(anyhow!(
            "remote config hash mismatch for {} / {} / {}",
            record.tool,
            record.category,
            record.rel_path
        ));
    }

    let existed = target_path.exists();
    let backup_path = if existed {
        Some(create_backup(target_path)?)
    } else {
        None
    };

    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(target_path, &record.content)?;

    let written = fs::read_to_string(target_path)?;
    let written_hash = ConfigItem::new(
        tool,
        category,
        record.rel_path.clone(),
        written,
        record.last_modified,
        record.is_device_specific,
    )
    .content_hash;
    if written_hash != record.content_hash {
        return Err(anyhow!(
            "written config hash mismatch for {} / {} / {}",
            record.tool,
            record.category,
            record.rel_path
        ));
    }

    Ok(PullApplyResult {
        created: !existed,
        backup_path,
    })
}

fn create_backup(path: &Path) -> Result<PathBuf> {
    if !path.is_file() {
        return Err(anyhow!(
            "cannot back up non-file path before pull: {}",
            path.display()
        ));
    }

    let suffix = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .with_context(|| format!("invalid backup target path {}", path.display()))?;
    let backup_path = path.with_file_name(format!("{file_name}.sync-devices.bak.{suffix}"));

    fs::copy(path, &backup_path)?;
    Ok(backup_path)
}

fn index_config_items(items: &[ConfigItem]) -> BTreeMap<(String, String, String), &ConfigItem> {
    items
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
        .collect()
}

const WORKER_SCRIPT_NAME: &str = "sync-devices-worker";
const KV_NAMESPACE_TITLE: &str = "sync-devices-configs";
const KV_BINDING_NAME: &str = "SYNC_CONFIGS";

async fn run_setup() -> Result<()> {
    // Step 1: Load existing session
    let store = session_store::SessionStore::new()?;
    let session = store
        .load()?
        .ok_or_else(|| anyhow!("Not logged in. Run `sync-devices login` first."))?;

    let cf = cloudflare_api::CloudflareApiClient::new(&session.api_token, &session.account_id);

    // Step 2: Create or find KV namespace
    println!("Creating KV namespace...");
    let (kv_ns, kv_created) = cf.ensure_kv_namespace(KV_NAMESPACE_TITLE).await?;
    if kv_created {
        println!("  Created KV namespace: {} ({})", kv_ns.title, kv_ns.id);
    } else {
        println!(
            "  KV namespace already exists: {} ({})",
            kv_ns.title, kv_ns.id
        );
    }

    // Step 3: Deploy Worker script with KV binding
    println!("Deploying Worker script...");
    cf.deploy_worker(
        WORKER_SCRIPT_NAME,
        worker_bundle::WORKER_JS,
        &kv_ns.id,
        KV_BINDING_NAME,
    )
    .await?;
    println!("  Worker deployed: {}", WORKER_SCRIPT_NAME);

    // Step 4: Enable workers.dev route
    println!("Enabling workers.dev route...");
    cf.enable_workers_dev_route(WORKER_SCRIPT_NAME).await?;

    // Step 5: Resolve Worker URL
    let worker_url = cf.resolve_worker_url(WORKER_SCRIPT_NAME).await?;
    println!("  Worker URL: {}", worker_url);

    // Step 6: Verify deployment by calling GET /
    println!("Verifying deployment...");
    let verify_resp = reqwest::get(&worker_url).await;
    match verify_resp {
        Ok(resp) if resp.status().is_success() => {
            println!("  Worker is responding.");
        }
        Ok(resp) => {
            println!(
                "  Warning: Worker responded with status {}. It may need a moment to propagate.",
                resp.status()
            );
        }
        Err(err) => {
            println!(
                "  Warning: Could not reach Worker ({}). It may need a moment to propagate.",
                err
            );
        }
    }

    // Step 7: Save Worker URL to session
    store.set_worker_url(&worker_url)?;
    println!("  Worker URL saved to session.");

    println!("\nSetup complete. You can now use `sync-devices push` and `sync-devices pull`.");
    Ok(())
}

async fn run_teardown() -> Result<()> {
    let store = session_store::SessionStore::new()?;
    let session = store
        .load()?
        .ok_or_else(|| anyhow!("Not logged in. Run `sync-devices login` first."))?;

    let cf = cloudflare_api::CloudflareApiClient::new(&session.api_token, &session.account_id);

    // Show what will be deleted
    println!("The following resources will be deleted from your Cloudflare account:");
    println!("  Worker script: {}", WORKER_SCRIPT_NAME);
    println!("  KV namespace:  {}", KV_NAMESPACE_TITLE);
    if let Some(url) = &session.worker_url {
        println!("  Worker URL:    {}", url);
    }
    println!();
    println!("All synced configuration data in KV will be permanently lost.");
    println!("Type 'yes' to confirm:");

    let confirmation = read_hidden_line()?;
    if confirmation != "yes" {
        println!("Teardown cancelled.");
        return Ok(());
    }

    // Delete Worker script first (it depends on the KV binding)
    println!("Deleting Worker script...");
    match cf.delete_worker(WORKER_SCRIPT_NAME).await {
        Ok(()) => println!("  Worker deleted."),
        Err(cloudflare_api::CloudflareApiError::Api { code: 10007, .. }) => {
            println!("  Worker not found (already deleted).");
        }
        Err(err) => return Err(err.into()),
    }

    // Delete KV namespace
    println!("Deleting KV namespace...");
    match cf.find_kv_namespace(KV_NAMESPACE_TITLE).await? {
        Some(ns) => {
            cf.delete_kv_namespace(&ns.id).await?;
            println!("  KV namespace deleted: {}", ns.id);
        }
        None => {
            println!("  KV namespace not found (already deleted).");
        }
    }

    // Clear worker_url from session
    store.set_worker_url("")?;
    println!("\nTeardown complete. Worker and KV namespace have been removed.");
    Ok(())
}

fn is_missing_session_error(error: &anyhow::Error) -> bool {
    matches!(
        error.downcast_ref::<transport::TransportError>(),
        Some(transport::TransportError::MissingSession)
    )
}

fn read_hidden_line() -> Result<String> {
    // Flush prompt so it appears before reading
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(prefix: &str) -> Result<Self> {
            let suffix = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
            let path = env::temp_dir().join(format!("sync-devices-{prefix}-{suffix}"));
            fs::create_dir_all(&path)?;
            Ok(Self { path })
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn build_remote_record(
        tool: model::Tool,
        category: model::Category,
        rel_path: &str,
        content: &str,
    ) -> transport::RemoteConfigRecord {
        let item = ConfigItem::new(
            tool,
            category,
            rel_path.to_string(),
            content.to_string(),
            42,
            false,
        );

        transport::RemoteConfigRecord {
            id: format!(
                "{}:{}:{}",
                tool.as_str(),
                category.as_str(),
                rel_path.replace('/', ":")
            ),
            tool: tool.as_str().to_string(),
            category: category.as_str().to_string(),
            rel_path: rel_path.to_string(),
            content: content.to_string(),
            content_hash: item.content_hash,
            last_modified: item.last_modified,
            device_id: "remote-device".to_string(),
            is_device_specific: item.is_device_specific,
            updated_at: 42,
        }
    }

    #[tokio::test]
    #[ignore = "requires a stored session and live backend access"]
    async fn live_push_command_uploads_and_cleans_temp_skill() -> Result<()> {
        if std::env::var("SYNC_DEVICES_RUN_LIVE_TESTS").as_deref() != Ok("1") {
            return Ok(());
        }

        let client = transport::ApiTransport::from_session_store()?;
        let suffix = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
        let rel_path = format!("skills/push-live-smoke-{suffix}/SKILL.md");
        let snapshot = adapter::build_local_snapshot(&[ConfigItem::new(
            model::Tool::SharedAgents,
            model::Category::Skills,
            rel_path.clone(),
            "token = \"sk-abcdefghijklmnopqrstuvwxyz123456\"\n".to_string(),
            (suffix / 1000) as u64,
            false,
        )])?;
        let result = push_local_changes(&snapshot).await?;
        assert_eq!(result.summary.uploaded, 1);
        assert_eq!(result.summary.created, 1);
        assert_eq!(result.summary.modified, 0);
        assert_eq!(result.uploaded_records.len(), 1);

        let uploaded_record = result.uploaded_records[0].clone();
        assert_eq!(uploaded_record.rel_path, rel_path);
        assert!(uploaded_record.content.contains("<REDACTED:api_key>"));
        assert_eq!(uploaded_record.device_id, snapshot.manifest.device_id);

        let deleted = client.delete_config(&uploaded_record.id).await?;
        assert_eq!(deleted.id, uploaded_record.id);

        Ok(())
    }

    #[test]
    fn apply_remote_record_to_path_creates_backup_before_overwrite() -> Result<()> {
        let temp_dir = TestDir::new("pull-backup")?;
        let target_path = temp_dir.path().join("config.toml");
        fs::write(&target_path, "before = true\n")?;
        let record = build_remote_record(
            model::Tool::Codex,
            model::Category::Settings,
            "config.toml",
            "after = true\n",
        );

        let applied = apply_remote_record_to_path(
            &record,
            model::Tool::Codex,
            model::Category::Settings,
            &target_path,
        )?;

        assert!(!applied.created);
        let backup_path = applied.backup_path.context("expected backup path")?;
        assert!(backup_path.exists());
        assert_eq!(fs::read_to_string(&backup_path)?, "before = true\n");
        assert_eq!(fs::read_to_string(&target_path)?, "after = true\n");

        Ok(())
    }

    #[test]
    fn apply_remote_record_to_path_creates_new_file_without_backup() -> Result<()> {
        let temp_dir = TestDir::new("pull-create")?;
        let target_path = temp_dir.path().join("nested").join("config.toml");
        let record = build_remote_record(
            model::Tool::Codex,
            model::Category::Settings,
            "nested/config.toml",
            "created = true\n",
        );

        let applied = apply_remote_record_to_path(
            &record,
            model::Tool::Codex,
            model::Category::Settings,
            &target_path,
        )?;

        assert!(applied.created);
        assert!(applied.backup_path.is_none());
        assert_eq!(fs::read_to_string(&target_path)?, "created = true\n");

        Ok(())
    }

    #[test]
    fn apply_remote_record_to_path_rejects_mismatched_remote_hash() {
        let temp_dir = TestDir::new("pull-hash").expect("temp dir");
        let target_path = temp_dir.path().join("config.toml");
        let mut record = build_remote_record(
            model::Tool::Codex,
            model::Category::Settings,
            "config.toml",
            "content = true\n",
        );
        record.content_hash = "not-a-real-hash".to_string();

        let error = apply_remote_record_to_path(
            &record,
            model::Tool::Codex,
            model::Category::Settings,
            &target_path,
        )
        .unwrap_err();

        assert!(error.to_string().contains("remote config hash mismatch"));
        assert!(!target_path.exists());
    }
}
