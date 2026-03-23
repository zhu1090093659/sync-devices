# sync-devices

Cross-platform CLI tool for syncing AI CLI tool configurations across devices.

Supports **Claude Code**, **Codex**, **Cursor**, and **shared agent skills** — keeping your settings, instructions, commands, skills, MCP configs, plugins, and rules in sync via a self-deployed Cloudflare Workers + KV backend, authenticated with your Cloudflare API Token.

## Features

- **Multi-tool support** — Claude Code (`~/.claude`), Codex (`~/.codex`), Cursor (`~/.cursor`), shared agents (`~/.agents`)
- **Selective sync** — choose which items to push or pull via TUI checkboxes
- **Conflict detection** — cross-device changes flagged as conflicts, resolved interactively
- **Sensitive data redaction** — API keys, tokens, and secrets are automatically redacted before upload
- **Incremental sync** — SHA-256 content hashing ensures only changed items are transferred
- **Device-specific items** — paths and environment variables are flagged for per-device handling
- **Interactive TUI** — tree-based config browser, unified diff view, conflict resolution, device management

## Installation

### Quick Install (recommended)

**macOS / Linux:**

```sh
curl -fsSL https://raw.githubusercontent.com/zhu1090093659/sync-devices/master/install.sh | sh
```

**Windows (PowerShell):**

```powershell
irm https://raw.githubusercontent.com/zhu1090093659/sync-devices/master/install.ps1 | iex
```

### Homebrew (macOS / Linux)

```bash
brew install zhu1090093659/tap/sync-devices
```

### Pre-built Binaries

Download the latest binary for your platform from [GitHub Releases](https://github.com/zhu1090093659/sync-devices/releases/latest):

| Platform | Binary |
|----------|--------|
| Linux x86_64 | `sync-devices-linux-x86_64` |
| Linux aarch64 | `sync-devices-linux-aarch64` |
| macOS x86_64 | `sync-devices-darwin-x86_64` |
| macOS aarch64 | `sync-devices-darwin-aarch64` |
| Windows x86_64 | `sync-devices-windows-x86_64.exe` |

### cargo-binstall (no compilation)

```bash
cargo binstall sync-devices
```

### From Source

```bash
cargo install --path .
```

## Prerequisites

You need a Cloudflare account with a free Workers plan. Create an API Token at [dash.cloudflare.com/profile/api-tokens](https://dash.cloudflare.com/profile/api-tokens) with the following permissions:

- **Account > Workers Scripts > Edit**
- **Account > Workers KV Storage > Edit**

## Quick Start

1. **Login** with your Cloudflare API Token:

   ```bash
   sync-devices login
   ```

   Paste your token when prompted (input is hidden).

2. **Deploy** the Worker to your Cloudflare account:

   ```bash
   sync-devices setup
   ```

   This creates a KV namespace and deploys the sync Worker automatically.

3. **Check status** to see local config items and remote diff:

   ```bash
   sync-devices status
   ```

4. **Push** local changes to the cloud:

   ```bash
   sync-devices push
   ```

5. **Pull** remote changes to this device:

   ```bash
   sync-devices pull
   ```

6. **Open the TUI** for interactive management:

   ```bash
   sync-devices manage
   ```

## CLI Commands

| Command | Description |
|---------|-------------|
| `login` | Authenticate with a Cloudflare API Token |
| `setup` | Deploy Worker and KV to your Cloudflare account |
| `logout` | Clear stored credentials |
| `push` | Upload local config changes to remote |
| `pull` | Download remote-only configs to local |
| `status` | Show local items, Worker info, and remote diff |
| `manage` | Open interactive TUI |
| `teardown` | Remove Worker and KV from your Cloudflare account |

## TUI Keybindings

| Key | Action |
|-----|--------|
| `Up/Down` | Navigate items |
| `Enter/Right` | Expand node or open diff view |
| `Left` | Collapse node or go to parent |
| `Space` | Toggle checkbox (batch on Tool/Category) |
| `a` | Select all / deselect all |
| `d` | Open diff view for selected item |
| `p` | Push checked items |
| `l` | Pull checked items |
| `r` | Open conflict resolution view |
| `i` | Open device info view |
| `q/Esc` | Back / quit |

## Architecture

```
sync-devices (Rust CLI)
    |
    +-- adapter/          Config adapters per tool (scan, resolve paths)
    |   +-- claude_code   ~/.claude settings, CLAUDE.md, commands, skills, plugins
    |   +-- codex         ~/.codex config.toml, AGENTS.md, rules, skills
    |   +-- cursor        ~/.cursor mcp.json, commands, rules
    |   +-- shared_agents ~/.agents skills
    |
    +-- sanitizer         Regex-based sensitive data detection and redaction
    +-- model             Data types, manifest diffing, push planning
    +-- transport         HTTP client with retry, health check, Bearer auth
    +-- auth              Cloudflare API Token verification
    +-- cloudflare_api    CF REST API client (deploy Worker, manage KV)
    +-- session_store     Keyring-based credential persistence
    +-- worker_bundle     Embedded Worker JS (include_str!)
    +-- tui               ratatui interactive interface
    |
    v
Cloudflare Workers + KV (TypeScript / Hono, self-deployed)
    +-- GET /             Worker self-description (version, kv_bound)
    +-- GET /healthz      Health check
    +-- /api/manifest     Sync manifest (GET)
    +-- /api/configs      Config CRUD (GET/PUT/DELETE)
```

## Backend

The Worker is embedded in the CLI binary and deployed to your own Cloudflare account via `sync-devices setup`. No shared infrastructure, no third-party secrets. For manual Worker development see `worker/`.

## Development

```bash
# Build the Worker bundle first (required before cargo build)
cd worker && npm install && npm run build && cd ..

# Build
cargo build

# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run -- status
```

## Security

All config content passes through the sanitizer before upload. The following patterns are detected and replaced with `<REDACTED:...>` placeholders:

- API keys (`sk-...`)
- ACE tokens (`ace_...`)
- GitHub PATs (`ghp_...`)
- GitHub OAuth tokens (`gho_...`)
- Bearer tokens
- Base64-encoded secrets (40+ chars)

Credentials are stored in the system keyring (Windows Credential Manager, macOS Keychain, or Linux Secret Service).

## License

MIT
