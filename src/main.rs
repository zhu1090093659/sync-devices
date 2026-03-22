mod adapter;
mod auth;
mod model;
mod sanitizer;
mod session_store;
mod transport;

use clap::{Parser, Subcommand};

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
    /// Login via GitHub OAuth
    Login,
    /// Logout and clear stored credentials
    Logout,
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Login => {
            let client = auth::DeviceFlowClient::from_env()?;
            let device_code = client.request_device_code().await?;

            println!("Open this URL in your browser:");
            println!("  {}", device_code.verification_uri);
            println!();
            println!("Enter this device code:");
            println!("  {}", device_code.user_code);
            println!();
            println!("Waiting for authorization via {} ...", client.base_url());

            let session = client.poll_for_session_token(&device_code).await?;
            let store = session_store::SessionStore::new()?;
            store.save(client.base_url().as_str(), &session)?;

            println!("Login succeeded.");
            println!("User: @{}", session.user.login);
            if let Some(name) = &session.user.name {
                println!("Name: {}", name);
            }
            println!("User ID: {}", session.user.id);
            println!("Avatar URL: {}", session.user.avatar_url);
            println!("Token type: {}", session.token_type);
            println!("Expires in: {} seconds", session.expires_in);
            if !session.scope.is_empty() {
                println!("Scope: {}", session.scope);
            }
            println!("Session token stored securely in the system keyring.");
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
            println!("Push not yet implemented");
        }
        Commands::Pull => {
            println!("Pull not yet implemented");
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

            match transport::ApiTransport::from_session_store() {
                Ok(client) => {
                    let session = client.get_session().await?;
                    let remote_manifest = client.get_manifest().await?;
                    let configs = client
                        .list_configs(transport::ConfigListFilters::default())
                        .await?;
                    let diff = model::diff_manifests(&local_manifest, &remote_manifest);
                    let diff_summary = model::summarize_manifest_diff(&diff);
                    println!();
                    println!("Remote session:");
                    println!("  User: @{}", session.user.login);
                    if let Some(subject) = session.token.subject {
                        println!("  Subject: {}", subject);
                    }
                    println!("Remote manifest items: {}", remote_manifest.items.len());
                    println!("Remote config records: {}", configs.len());
                    println!("Manifest diff summary:");
                    println!("  Local only: {}", diff_summary.local_only);
                    println!("  Remote only: {}", diff_summary.remote_only);
                    println!("  Modified: {}", diff_summary.modified);
                    println!("  Conflict: {}", diff_summary.conflict);
                    println!("  Unchanged: {}", diff_summary.unchanged);
                }
                Err(transport::TransportError::MissingSession) => {
                    println!();
                    println!("Remote session unavailable: not logged in.");
                }
                Err(error) => return Err(error.into()),
            }
        }
        Commands::Manage => {
            println!("TUI not yet implemented");
        }
        Commands::Diff { tool } => {
            println!("Diff for {} not yet implemented", tool);
        }
    }

    Ok(())
}
