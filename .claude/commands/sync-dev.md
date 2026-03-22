# sync-devices Development SKILL

You are developing **sync_devices** — a cross-platform Rust CLI tool with TUI for syncing AI CLI tool configurations (Claude Code, Codex, Cursor) across devices via Cloudflare Workers + KV.

## First Action: Read Progress

**MANDATORY**: Before doing anything else, read `docs/progress/MASTER.md` to understand:
1. Which phase you are currently in
2. Which tasks are completed
3. What the next steps are

Then read the specific phase progress file for the active phase.

## Tech Stack

- **Language**: Rust (edition 2021)
- **TUI**: ratatui + crossterm
- **HTTP**: reqwest + tokio
- **Backend**: Cloudflare Workers + KV (TypeScript)
- **Auth**: GitHub OAuth Device Flow
- **Serialization**: serde + serde_json + toml
- **Diff**: similar crate

## Coding Standards

- All code comments in English
- Follow Rust 2021 idioms (use `?` for error propagation, avoid `.unwrap()` in production code)
- Use `thiserror` for custom error types
- Use `clap` derive macros for CLI argument parsing
- Module organization: one file per logical module, avoid deep nesting
- Prefer `impl Trait` over `dyn Trait` where possible
- Cyclomatic complexity per function: keep under 10
- For Cloudflare Workers code: TypeScript, use Hono framework for routing

## Architecture Key Decisions

- **Config Adapter pattern**: Each tool (Claude Code, Codex, Cursor) has its own Adapter implementing a shared trait. This isolates tool-specific parsing logic.
- **Sanitizer**: All config content passes through a Sanitizer before upload. It detects and redacts API keys, tokens, and sensitive environment variables.
- **Incremental sync**: Use content SHA-256 hashes. Only push/pull items whose hashes differ between local and remote manifests.
- **Device-specific items**: Some config fields (local paths, environment variables) are marked as device-specific and handled with path mapping or user confirmation.

## After Completing Each Task

1. Update the checkbox in the relevant `docs/progress/phase-N-*.md` file: `- [ ]` → `- [x]`
2. Update the progress count in `docs/progress/MASTER.md` table (e.g., `1/8` → `2/8`)
3. Update the "Current Status" section in MASTER.md
4. If all tasks in a phase are done, mark the phase status as "已完成" in MASTER.md

## When All Tasks Are Done

When every checkbox across all phase files is checked, initiate cleanup:
1. Announce completion to the user
2. Ask which docs/artifacts to keep
3. Remove artifacts the user doesn't want to keep

$ARGUMENTS
