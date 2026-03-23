# sync-devices 项目架构概览

本文档描述 sync-devices 项目的当前架构、技术栈、构建体系，以及即将进行的架构转型方向。

---

## 项目定位

sync-devices 是一个跨平台的命令行工具，用于在多台设备之间同步 AI 工具的配置文件。目前支持 Claude Code、Codex、Cursor 和 SharedAgents 四种工具。整个系统由两部分组成：一个 Rust 编写的 CLI 客户端，以及一个部署在 Cloudflare Workers 上的 TypeScript 后端。

---

## 系统架构（当前）

```
                             Internet
                                |
  +----------------------------+----------------------------+
  |          Cloudflare Workers (shared instance)           |
  |  sync-devices-worker.1090093659.workers.dev             |
  |                                                         |
  |  +-------------+  +-----------+  +-------------------+  |
  |  | index.ts    |  | auth.ts   |  | github-oauth.ts   |  |
  |  | Hono Router |  | JWT HS256 |  | Device Flow Proxy |  |
  |  +------+------+  +-----+-----+  +---------+---------+  |
  |         |               |                   |            |
  |         v               v                   v            |
  |  +-------------------+        +--------------------+     |
  |  | config-store.ts   |        |   GitHub OAuth API |     |
  |  | KV CRUD + Manifest|        | (api.github.com)   |     |
  |  +--------+----------+        +--------------------+     |
  |           |                                              |
  |           v                                              |
  |  +------------------+                                    |
  |  | SYNC_CONFIGS KV  |                                    |
  |  +------------------+                                    |
  +---------------------------------------------------------+
                                |
                           HTTPS / Bearer JWT
                                |
  +---------------------------------------------------------+
  |               CLI (Rust binary)                         |
  |                                                         |
  |  +----------+  +-----------+  +----------------------+  |
  |  | main.rs  |  | auth.rs   |  | transport.rs         |  |
  |  | clap CLI |  | Device    |  | reqwest HTTP client  |  |
  |  | commands |  | Flow      |  | retry + bearer auth  |  |
  |  +----+-----+  +-----+-----+  +----------+----------+  |
  |       |              |                    |              |
  |       v              v                    v              |
  |  +-----------+  +-----------------+  +---------------+  |
  |  | model.rs  |  | session_store.rs|  | adapter/      |  |
  |  | domain    |  | OS keyring      |  | config scan   |  |
  |  | types     |  | (keyring crate) |  | per tool      |  |
  |  +-----------+  +-----------------+  +-------+-------+  |
  |       |                                      |          |
  |       v                                      v          |
  |  +-----------+                     +---------------+    |
  |  | tui.rs    |                     | sanitizer.rs  |    |
  |  | ratatui   |                     | regex redact  |    |
  |  | TUI mgmt  |                     +---------------+    |
  |  +-----------+                                          |
  +---------------------------------------------------------+
                                |
              Local filesystem (~/.claude, ~/.codex, etc.)
```

### 数据流概述

CLI 启动后，adapter 模块扫描本地各 AI 工具的配置目录，生成 `ConfigItem` 列表。sanitizer 对敏感内容做正则脱敏后，model 模块计算出 `SyncManifest`。transport 模块携带 JWT Bearer Token 与 Worker 通信，执行 push（上传变更）或 pull（拉取远端配置）。session_store 使用操作系统的 keyring 持久化认证凭据。tui 模块提供交互式的配置浏览、差异对比和冲突解决界面。

认证流程是 GitHub OAuth Device Flow：CLI 调用 Worker 的 `/api/auth/device/code` 获取 user_code，用户在浏览器完成授权后，CLI 轮询 `/api/auth/device/token`，Worker 从 GitHub 换取 access_token 后签发 JWT 返回客户端。

---

## 技术栈

### CLI 端 (Rust)

| 用途 | crate | 版本 |
|------|-------|------|
| CLI 框架 | clap (derive) | 4 |
| 异步运行时 | tokio (full) | 1 |
| HTTP 客户端 | reqwest (rustls-tls) | 0.12 |
| 序列化 | serde / serde_json / toml | 1 / 1 / 0.8 |
| TUI 终端界面 | ratatui + crossterm | 0.29 / 0.28 |
| 文本差异 | similar | 2 |
| 错误处理 | thiserror + anyhow | 2 / 1 |
| 哈希 | sha2 | 0.10 |
| 目录路径 | dirs | 6 |
| 正则表达式 | regex | 1 |
| 时间处理 | chrono (serde) | 0.4 |
| 系统密钥存储 | keyring | 3 (平台特定 features) |

keyring 的 feature 按平台分化：Windows 用 `windows-native`，macOS 用 `apple-native`，Linux 用 `linux-native-sync-persistent`。HTTP 层选用 `rustls-tls` 而非 `native-tls`，避免对系统 OpenSSL 的依赖，简化交叉编译。

### Worker 端 (TypeScript / Cloudflare Workers)

| 用途 | 包 | 版本 |
|------|-----|------|
| Web 框架 | hono | ^4.0 |
| JWT 签发/验证 | hono/jwt (HS256) | 内置 |
| 构建/部署 | wrangler | ^4.0 |
| 类型检查 | typescript | ^5.0 |

Worker 使用单一 KV namespace `SYNC_CONFIGS` 存储全部配置数据，key 格式为 `configs:{ownerSubject}:{tool}:{category}:{encodedPath}`。

---

## 入口点

### CLI

- 二进制入口：`src/main.rs` 中的 `#[tokio::main] async fn main()`
- 命令解析通过 clap derive 宏生成的 `Cli` 和 `Commands` enum
- 主要命令：`login`、`logout`、`status`、`push`、`pull`、`diff`、`manage`

### Worker

- 入口文件：`worker/src/index.ts`，导出 Hono app 实例
- 公开路由：`/`、`/healthz`、`/api/auth/device/code`、`/api/auth/device/token`
- 受保护路由（需 JWT）：`/api/session`、`/api/configs`、`/api/manifest`、`/api/configs/:tool/:category/*`（PUT）、`/api/configs/:id`（DELETE）

---

## 构建与发布体系

CLI 通过 GitHub Actions CI 构建，覆盖 5 个目标平台：

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

aarch64 Linux 采用多架构交叉编译方案（在 CI 中安装 aarch64 工具链和对应的 libdbus-1-dev）。二进制分发支持 `cargo-binstall`，`Cargo.toml` 中的 `[package.metadata.binstall]` 配置了各平台的下载 URL 模板。

Worker 通过 `wrangler deploy` 部署至 Cloudflare Workers。部署前会运行 `scripts/verify-deploy-config.mjs` 做配置校验。KV namespace 通过 `wrangler kv namespace create` 命令创建。

---

## 部署模型对比

### 当前模型：共享 Worker

所有用户连接同一个 Worker 实例（`sync-devices-worker.1090093659.workers.dev`）。认证依赖 GitHub OAuth，Worker 持有 `GITHUB_CLIENT_ID` 和 `JWT_SECRET`。用户数据通过 JWT 中的 `sub` claim（格式 `github:{userId}`）隔离，在 KV key 前缀中区分不同用户。

这个模型的问题在于：所有人的数据存在同一个 KV namespace 中；需要维护一个公共 GitHub OAuth App；中国地区访问 `.workers.dev` 域名需要代理。

### 目标模型：自部署 Worker

每个用户将 Worker 部署到自己的 Cloudflare 账户。CLI 内嵌 Worker 的 JS 源码，提供一键部署能力。认证从 GitHub OAuth 切换为 Cloudflare API Token 验证——用户提供一个具有 Workers + KV 权限的 API Token，CLI 使用该 Token 完成 Worker 部署和后续 API 调用。

这个模型消除了对公共基础设施的依赖，每个用户独占自己的 KV namespace。同时可以绑定自定义域名，绕过 `.workers.dev` 的网络限制。

转型涉及的核心变更包括：

1. 删除 GitHub OAuth 相关的全部代码（CLI 端 auth.rs、Worker 端 github-oauth.ts）
2. 用 Cloudflare API Token 验证替代 JWT 认证
3. CLI 新增 Worker 部署流程（内嵌 JS、调用 CF API 创建 Worker 和 KV）
4. 修复 manifest 端点的 KV 性能问题（当前实现在生成 manifest 时并行读取所有 config record，配置数量多时会触发 Worker 超时）
5. session_store 从存储 JWT 改为存储 CF API Token 和 Worker URL
