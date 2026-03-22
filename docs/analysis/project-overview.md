# Project Overview: sync_devices

## 项目定位

一个基于 Rust 的跨平台 CLI 工具，通过交互式 TUI 界面，让开发者在多台设备间同步和管理 AI CLI 工具（Claude Code、Codex、Cursor）的配置信息。

## 技术栈

| 组件 | 技术选型 | 说明 |
|------|----------|------|
| 语言 | Rust | 跨平台编译，无运行时依赖 |
| TUI 框架 | ratatui + crossterm | Rust 生态最成熟的 TUI 方案 |
| HTTP 客户端 | reqwest | 异步 HTTP，支持 WASM |
| 序列化 | serde + serde_json + toml | 解析 JSON/TOML 配置 |
| 文本差异 | similar | 高性能 diff 引擎 |
| 异步运行时 | tokio | 网络 IO |
| 后端 | Cloudflare Workers + KV | Serverless API + KV 存储 |
| 认证 | GitHub OAuth Device Flow | CLI 友好的 OAuth 流程 |

## 系统架构

```
┌──────────────────────────────────────────────┐
│                  CLI / TUI                    │
│  login │ push │ pull │ status │ manage │ diff │
├──────────────────────────────────────────────┤
│              Sync Engine                      │
│  scanner → hasher → differ → resolver         │
├──────────────────────────────────────────────┤
│           Config Adapters                     │
│  Claude Code │ Codex │ Cursor                 │
├──────────────────────────────────────────────┤
│           Transport Layer                     │
│  HTTP Client → CF Workers API                 │
├──────────────────────────────────────────────┤
│         Cloudflare Backend                    │
│  Workers (API) + KV (Storage)                 │
└──────────────────────────────────────────────┘
```

## 核心命令

| 命令 | 功能 |
|------|------|
| `sync-devices login` | GitHub OAuth 登录，获取并缓存 token |
| `sync-devices push` | 扫描本地配置，上传至云端 |
| `sync-devices pull` | 从云端拉取配置，TUI 对比后选择性应用 |
| `sync-devices status` | 显示本地与云端的差异摘要 |
| `sync-devices manage` | 进入 TUI 管理界面，浏览/编辑/删除已同步的配置 |
| `sync-devices diff <tool>` | 显示指定工具的本地 vs 云端差异 |

## 目标平台

- Windows (x86_64)
- macOS (x86_64 + aarch64)
- Linux (x86_64)

## 分发渠道

- `cargo install sync-devices`
- GitHub Releases（预编译二进制）
- Homebrew (macOS/Linux)
