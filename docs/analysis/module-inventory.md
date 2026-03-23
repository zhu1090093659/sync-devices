# sync-devices 模块清单

本文档逐模块梳理项目的职责、依赖关系、规模和转型影响。复杂度评级基于圈复杂度和业务逻辑密度的主观判断。转型影响评级定义如下：

- **None** -- 无需任何改动
- **Low** -- 少量适配性修改（改引用、改显示文本等）
- **Medium** -- 需要修改部分核心逻辑
- **High** -- 大规模重写或重新设计
- **Replace** -- 整个模块将被替换为新实现
- **Delete** -- 直接删除

---

## CLI 端 (Rust)

### src/main.rs

| 属性 | 说明 |
|------|------|
| **职责** | CLI 入口与命令编排。定义 clap 命令（login/logout/status/push/pull/diff/manage），实现 push 和 pull 的完整业务流程，包括本地扫描、远端对比、冲突检测、文件写入和备份。 |
| **LOC** | ~705 |
| **复杂度** | Medium -- push/pull 流程涉及多步骤编排，`apply_remote_record_to_path` 含文件 I/O 和哈希校验逻辑 |
| **内部依赖** | auth, transport, session_store, model, adapter, sanitizer, tui |
| **外部依赖** | clap, tokio, anyhow, serde_json |
| **关键符号** | `Cli`, `Commands` (enum), `push_local_changes`, `pull_remote_changes`, `fetch_remote_records`, `apply_remote_record_to_path`, `PushSummary`, `PullSummary` |
| **转型影响** | **Medium** -- login 命令需从 Device Flow 改为 CF Token 输入流程；push/pull 的传输层调用不变但 session 初始化逻辑会变；可能新增 `deploy` 子命令 |

### src/auth.rs

| 属性 | 说明 |
|------|------|
| **职责** | GitHub OAuth Device Flow 客户端。向 Worker 请求 device_code，轮询等待用户授权，最终获取 SessionTokenResponse（内含 JWT）。 |
| **LOC** | ~215 |
| **复杂度** | Medium -- 轮询循环含超时、slow_down 退避、多种终止条件 |
| **内部依赖** | 无 |
| **外部依赖** | reqwest, serde, serde_json, tokio, thiserror |
| **关键符号** | `DeviceFlowClient`, `DeviceCodeResponse`, `SessionTokenResponse`, `SessionUser`, `AuthClientError`, `SessionPollState` |
| **关键常量** | `DEFAULT_API_BASE_URL = "https://sync-devices-worker.1090093659.workers.dev"` |
| **转型影响** | **Replace** -- GitHub OAuth 流程将被完全移除。新模块需实现 CF API Token 验证（调用 `https://api.cloudflare.com/client/v4/user/tokens/verify`）和 Worker 部署流程。`SessionTokenResponse` 和 `SessionUser` 结构体将不再需要，取而代之的是 CF Token 和 Worker URL 的简单存储。 |

### src/transport.rs

| 属性 | 说明 |
|------|------|
| **职责** | HTTP 传输层。封装 reqwest 客户端，提供 Bearer Token 认证、自动重试（3 次、500ms 间隔）、连接/请求超时。暴露 `get_session`、`get_manifest`、`list_configs`、`upload_config`、`delete_config` 等高级 API。 |
| **LOC** | ~537 |
| **复杂度** | High -- 重试逻辑、错误映射、泛型 JSON 发送、多个业务端点方法 |
| **内部依赖** | session_store, model |
| **外部依赖** | reqwest, serde, serde_json, chrono, thiserror |
| **关键符号** | `ApiTransport`, `TransportError`, `RemoteConfigRecord`, `ConfigUploadRequest`, `SessionResponse`, `SessionMetadata` |
| **关键常量** | `MAX_ATTEMPTS = 3`, `RETRY_DELAY_MS = 500`, `CONNECT_TIMEOUT_SECS = 10`, `REQUEST_TIMEOUT_SECS = 30` |
| **转型影响** | **High** -- 认证方式从 JWT Bearer 改为 CF API Token（或直接的 token 头），`from_session_store` 构造逻辑需适配新的 session 格式。`get_session` 端点可能被简化或移除。可能需新增 Worker 部署相关的 HTTP 调用（CF REST API）。现有的 config CRUD 方法签名大体不变，但 base_url 将指向用户自己的 Worker。 |

### src/session_store.rs

| 属性 | 说明 |
|------|------|
| **职责** | 使用 OS keyring 持久化会话凭据。将 `StoredSession`（含 JWT、用户信息、API base URL）序列化为 JSON 存入系统密钥链。 |
| **LOC** | ~83 |
| **复杂度** | Low -- 简单的 CRUD 操作加 keyring 错误处理 |
| **内部依赖** | auth (`SessionTokenResponse`, `SessionUser`) |
| **外部依赖** | keyring, serde, serde_json, thiserror |
| **关键符号** | `SessionStore`, `StoredSession`, `SessionStoreError` |
| **转型影响** | **Medium** -- `StoredSession` 结构体需重新设计，从 JWT + GitHub 用户信息改为 CF API Token + Worker URL + Account ID 等字段。save/load/clear 接口本身不变，但序列化的数据模型会完全不同。对 `auth::SessionTokenResponse` 的依赖需要解耦。 |

### src/model.rs

| 属性 | 说明 |
|------|------|
| **职责** | 领域模型。定义 `Tool`/`Category` 枚举、`ConfigItem`（本地配置项）、`SyncManifest`/`ManifestEntry`（同步清单）。实现 manifest diff 算法（local vs remote 对比）、push plan 构建、哈希计算。 |
| **LOC** | ~748 |
| **复杂度** | Medium -- diff 算法和 push plan 构建有一定组合逻辑，但都是纯函数，测试覆盖充分 |
| **内部依赖** | 无 |
| **外部依赖** | serde, sha2, chrono |
| **关键符号** | `Tool`, `Category`, `ConfigItem`, `SyncManifest`, `ManifestEntry`, `ManifestDiffEntry`, `DiffStatus`, `PushPlanItem`, `diff_manifests`, `build_push_plan`, `compute_hash` |
| **转型影响** | **Low** -- 领域模型本身不受认证方式变化的影响。如果 KV 存储的 manifest 格式调整（例如新增 metadata 字段），可能需要扩展 `ManifestEntry`，但属于增量改动。 |

### src/adapter/ (mod.rs + 4 个子模块)

| 属性 | 说明 |
|------|------|
| **职责** | 配置文件扫描。`ConfigAdapter` trait 定义扫描接口，4 个子模块（claude_code.rs、codex.rs、cursor.rs、shared_agents.rs）各自实现对应工具的目录发现和文件读取。mod.rs 聚合所有 adapter 的扫描结果，执行 sanitizer 脱敏，构建 LocalSnapshot。 |
| **LOC** | ~353（mod.rs ~225 含测试，各子模块 ~30-50） |
| **复杂度** | Low -- 目录遍历和文件读取，逻辑直白 |
| **内部依赖** | model, sanitizer |
| **外部依赖** | anyhow, dirs |
| **关键符号** | `ConfigAdapter` (trait), `LocalSnapshot`, `scan_all`, `scan_local_snapshot`, `resolve_local_path` |
| **转型影响** | **None** -- 配置扫描逻辑完全独立于认证和传输层，无需任何改动 |

### src/sanitizer.rs

| 属性 | 说明 |
|------|------|
| **职责** | 敏感信息脱敏。使用预编译的正则模式列表检测 API key、GitHub PAT、Bearer token、Base64 secret 等，提供 scan（检测）和 redact（替换为 `<REDACTED:label>`）两个入口。 |
| **LOC** | ~184 |
| **复杂度** | Low -- 纯函数，正则匹配 + 字符串替换 |
| **内部依赖** | 无 |
| **外部依赖** | regex |
| **关键符号** | `SensitivePattern`, `ScanResult`, `Finding`, `scan`, `redact`, `PATTERNS` |
| **转型影响** | **None** -- 完全独立的工具模块。如果新增 CF API Token 的脱敏规则，可以作为增量添加，但不是转型的必要部分。 |

### src/tui.rs

| 属性 | 说明 |
|------|------|
| **职责** | 基于 ratatui 的交互式 TUI。实现树状配置浏览（按 Tool > Category > File 层级展开）、逐行文本 diff 查看、冲突解决（keep local / keep remote）、设备信息展示。支持键盘导航、分页、全选等操作。 |
| **LOC** | ~1459 |
| **复杂度** | High -- 状态机驱动的 UI 渲染，多种视图模式（Browse/Diff/Resolve/Devices），事件处理链复杂 |
| **内部依赖** | model, transport, adapter |
| **外部依赖** | ratatui, crossterm, similar, anyhow, tokio |
| **关键符号** | `App`, `ViewMode`, `TreeItem`, `DiffLine`, `Row`, `DiffViewState`, `ResolveState`, `run_manage`, `run_app`, `render` |
| **转型影响** | **Low** -- 主要影响在状态栏的用户信息显示（从 GitHub login 改为 CF account）。browse/diff/resolve 的核心逻辑不受影响。`execute_push`/`execute_pull` 的传输层调用通过 `ApiTransport` 间接完成，接口不变。 |

---

## Worker 端 (TypeScript)

### worker/src/index.ts

| 属性 | 说明 |
|------|------|
| **职责** | Hono 路由器。注册所有 HTTP 端点，包括公开的健康检查和 Device Flow 端点、受 JWT 保护的配置 CRUD 端点。实现请求解析、错误处理中间件、统一 JSON 响应格式。 |
| **LOC** | ~298 |
| **复杂度** | Medium -- 路由编排、错误分类处理、JWT payload 提取 |
| **内部依赖** | auth, github-oauth, config-store |
| **外部依赖** | hono |
| **关键符号** | `app` (Hono router), `protectedApi`, `readDeviceCode`, `requireOwnerSubject`, `handleApiError` |
| **转型影响** | **High** -- 需要移除 Device Flow 的两个端点（`/api/auth/device/code`、`/api/auth/device/token`）。JWT 认证中间件替换为 CF API Token 验证。`requireOwnerSubject` 逻辑简化（单用户场景下可能不需要 owner 隔离）。健康检查和 config CRUD 端点保留。 |

### worker/src/auth.ts

| 属性 | 说明 |
|------|------|
| **职责** | JWT 认证。签发 HS256 JWT（包含 GitHub 用户信息作为 claims），提供 Hono 中间件进行 JWT 验证，从 payload 中提取 SessionUser。 |
| **LOC** | ~182 |
| **复杂度** | Medium -- JWT 签发/验证、claims 构建、类型安全的 payload 提取 |
| **内部依赖** | github-oauth (`Env`, `GitHubAuthenticatedUser`, `ConfigurationError`) |
| **外部依赖** | hono, hono/jwt |
| **关键符号** | `issueSessionToken`, `jwtAuthMiddleware`, `readSessionUser`, `SessionTokenResponse`, `SessionUser`, `SessionClaims` |
| **关键常量** | `DEFAULT_JWT_TTL_SECONDS = 604800` (7 天) |
| **转型影响** | **Replace** -- JWT 认证将被 CF API Token 验证替代。新的认证中间件只需验证请求中携带的 token 是否为有效的 Cloudflare API Token（可以通过调用 CF API 验证，或者在自部署场景下简化为共享 secret 校验）。整个 JWT 签发逻辑、GitHub 用户 claims 体系都不再需要。 |

### worker/src/github-oauth.ts

| 属性 | 说明 |
|------|------|
| **职责** | GitHub OAuth Device Flow 代理。封装对 GitHub 的三个外部请求：请求 device_code、用 device_code 换取 access_token、用 access_token 获取用户资料。定义 `Env` 接口（Worker 的 binding 类型）和错误类型。 |
| **LOC** | ~209 |
| **复杂度** | Medium -- 外部 HTTP 调用、错误映射、响应解析 |
| **内部依赖** | 无（是其他模块的依赖源头） |
| **外部依赖** | Cloudflare Workers runtime (fetch) |
| **关键符号** | `Env` (binding 接口), `GitHubAuthenticatedUser`, `ConfigurationError`, `UpstreamRequestError`, `requestDeviceCode`, `exchangeDeviceCode`, `fetchAuthenticatedUser` |
| **转型影响** | **Delete** -- 这个文件将被完全删除。注意 `Env` 接口和 `ConfigurationError` 类被其他模块引用，需要搬迁到独立的 types 文件或内联到 config-store 中。 |

### worker/src/config-store.ts

| 属性 | 说明 |
|------|------|
| **职责** | KV 存储层。实现配置记录的 CRUD 操作和 manifest 生成。包含严格的输入验证（tool/category 枚举校验、路径规范化、content_hash 一致性校验）和 KV key 构建逻辑。 |
| **LOC** | ~501 |
| **复杂度** | High -- 分页 list + 并行 get（性能隐患）、SHA-256 哈希计算、多层校验、key/id 编解码 |
| **内部依赖** | github-oauth (`Env`, `ConfigurationError`) |
| **外部依赖** | Cloudflare Workers runtime (crypto.subtle) |
| **关键符号** | `StoredConfigRecord`, `ConfigUploadPayload`, `SyncManifestRecord`, `ManifestEntryRecord`, `RequestValidationError`, `saveConfigRecord`, `listConfigRecords`, `getConfigManifest`, `deleteConfigRecord` |
| **转型影响** | **Medium** -- 核心 CRUD 逻辑保留，但存在必须修复的性能缺陷。当前 `getConfigManifest` 调用 `listConfigRecords`，后者在 `do...while` 循环中对每批 keys 做 `Promise.all` 并行 get。当配置数量超过 100 条时，大量并行 KV 读取会触发 Worker CPU/wall-time 超时。修复方案可能包括：在 KV value 中内联 manifest 所需字段（避免二次读取）、维护独立的 manifest 缓存 key、或改用批量限流。此外 `Env` 接口的 import 来源需要从 github-oauth 迁移。 |

---

## 转型影响汇总矩阵

| 模块 | LOC | 复杂度 | 转型影响 | 改动类型 |
|------|-----|--------|----------|----------|
| src/main.rs | 705 | Medium | Medium | 修改 login 流程，可能新增 deploy 命令 |
| src/auth.rs | 215 | Medium | Replace | 整体替换为 CF Token 验证 |
| src/transport.rs | 537 | High | High | 重构认证方式，可能新增 CF API 调用 |
| src/session_store.rs | 83 | Low | Medium | 更换存储数据模型 |
| src/model.rs | 748 | Medium | Low | 可能扩展 ManifestEntry 字段 |
| src/adapter/ | 353 | Low | None | 无需改动 |
| src/sanitizer.rs | 184 | Low | None | 无需改动 |
| src/tui.rs | 1459 | High | Low | 更新状态栏显示 |
| worker/src/index.ts | 298 | Medium | High | 移除 OAuth 端点，替换认证中间件 |
| worker/src/auth.ts | 182 | Medium | Replace | 整体替换为 CF Token 认证 |
| worker/src/github-oauth.ts | 209 | Medium | Delete | 整个文件删除 |
| worker/src/config-store.ts | 501 | High | Medium | 修复 KV 性能问题，迁移类型依赖 |

按影响范围估算，约 1200 行 Rust 代码和 700 行 TypeScript 代码需要修改或重写，占总代码量的 40% 左右。其中 adapter、sanitizer、model 三个模块构成了稳定的核心，不会被转型波及。
