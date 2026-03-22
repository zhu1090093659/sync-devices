# Task Breakdown: sync_devices

## Phase 1: 项目骨架与核心数据模型

建立 Rust 项目结构、定义核心数据类型、实现配置文件扫描框架。

| # | 任务 | 优先级 | 工作量 | 依赖 | 验收标准 |
|---|------|--------|--------|------|----------|
| 1.1 | 初始化 Rust 项目（cargo init, 依赖声明, 目录结构） | P0 | S | 无 | `cargo build` 通过 |
| 1.2 | 定义核心数据模型（ConfigItem, Tool, Category, SyncManifest） | P0 | S | 1.1 | 模型可序列化/反序列化 |
| 1.3 | 实现 Config Adapter trait 和三个工具的 Adapter 骨架 | P0 | M | 1.2 | 每个 Adapter 能扫描并列出可同步项 |
| 1.4 | 实现 Claude Code Adapter（扫描 ~/.claude/ 下所有可同步文件） | P0 | M | 1.3 | 正确识别 settings.json, CLAUDE.md, commands/, skills/, plugins 配置 |
| 1.5 | 实现 Codex Adapter（扫描 ~/.codex/） | P0 | M | 1.3 | 正确识别 config.toml, AGENTS.md, rules/, skills/ |
| 1.6 | 实现 Cursor Adapter（扫描 ~/.cursor/） | P0 | S | 1.3 | 正确识别 mcp.json, commands/, rules/ |
| 1.7 | 实现 Shared Agents Adapter（扫描 ~/.agents/） | P1 | S | 1.3 | 正确识别 skills/ |
| 1.8 | 实现敏感信息扫描器（Sanitizer） | P0 | L | 1.2 | 能检测并替换 API Key、Token 等敏感模式 |

---

## Phase 2: 后端服务（Cloudflare Workers + KV）

实现云端 API 和存储层。

| # | 任务 | 优先级 | 工作量 | 依赖 | 验收标准 |
|---|------|--------|--------|------|----------|
| 2.1 | 搭建 CF Workers 项目结构（wrangler init） | P0 | S | 无 | `wrangler dev` 本地运行 |
| 2.2 | 实现 GitHub OAuth Device Flow 后端（token 交换端点） | P0 | M | 2.1 | 能完成 device code → access token 交换 |
| 2.3 | 实现 JWT 认证中间件（验证请求身份） | P0 | M | 2.2 | 未授权请求返回 401 |
| 2.4 | 实现配置上传 API（PUT /api/configs/:tool/:category/:path） | P0 | M | 2.3 | 配置正确存入 KV |
| 2.5 | 实现配置拉取 API（GET /api/configs） | P0 | M | 2.3 | 能按 tool/category 过滤返回配置 |
| 2.6 | 实现配置删除 API（DELETE /api/configs/:id） | P1 | S | 2.3 | 配置从 KV 中删除 |
| 2.7 | 实现配置元数据 API（GET /api/manifest）— 返回所有配置项的 hash 和时间戳 | P0 | S | 2.4 | 返回完整的 manifest |
| 2.8 | 部署脚本和 KV namespace 配置 | P1 | S | 2.1 | `wrangler deploy` 成功 |

---

## Phase 3: 认证与传输层（Rust CLI 端）

实现 CLI 端的登录、token 管理、HTTP 通信。

| # | 任务 | 优先级 | 工作量 | 依赖 | 验收标准 |
|---|------|--------|--------|------|----------|
| 3.1 | 实现 GitHub OAuth Device Flow 客户端 | P0 | M | 2.2 | CLI 执行 login 后获得 access token |
| 3.2 | 实现 token 本地安全存储（keyring 或加密文件） | P0 | M | 3.1 | token 不以明文存储 |
| 3.3 | 实现 HTTP Transport 层（封装 reqwest，处理认证头、重试、错误） | P0 | M | 3.1 | 能调通后端所有 API |
| 3.4 | 实现 `sync-devices login` 命令 | P0 | S | 3.1, 3.2 | 用户能完成登录流程 |
| 3.5 | 实现 `sync-devices logout` 命令 | P1 | S | 3.2 | 清除本地 token |

---

## Phase 4: 同步引擎

实现 diff、merge、push/pull 核心逻辑。

| # | 任务 | 优先级 | 工作量 | 依赖 | 验收标准 |
|---|------|--------|--------|------|----------|
| 4.1 | 实现本地 manifest 生成（扫描 → hash → manifest） | P0 | M | Phase 1 | 生成的 manifest 准确反映本地配置状态 |
| 4.2 | 实现 remote manifest 拉取与本地 manifest 对比 | P0 | M | 4.1, Phase 3 | 正确识别 new/modified/deleted/unchanged 状态 |
| 4.3 | 实现 push 逻辑（上传新增/修改的配置） | P0 | M | 4.2 | 增量推送，只上传有变化的项 |
| 4.4 | 实现 pull 逻辑（下载并应用配置，带备份） | P0 | L | 4.2 | 应用前备份原文件，应用后验证 hash |
| 4.5 | 实现冲突检测（本地和远端都有修改） | P0 | M | 4.2 | 正确识别冲突项 |
| 4.6 | 实现 `sync-devices push` 命令 | P0 | S | 4.3 | 命令行交互完整 |
| 4.7 | 实现 `sync-devices pull` 命令 | P0 | S | 4.4 | 命令行交互完整 |
| 4.8 | 实现 `sync-devices status` 命令 | P0 | S | 4.2 | 清晰展示差异摘要 |

---

## Phase 5: TUI 交互界面

构建交互式终端管理界面。

| # | 任务 | 优先级 | 工作量 | 依赖 | 验收标准 |
|---|------|--------|--------|------|----------|
| 5.1 | 搭建 ratatui 应用骨架（App struct, 事件循环, 基础布局） | P0 | M | Phase 1 | 能启动空白 TUI 窗口 |
| 5.2 | 实现配置浏览视图（按 tool/category 树形展示） | P0 | L | 5.1, Phase 4 | 能浏览所有已扫描的配置项 |
| 5.3 | 实现 Diff 视图（并排对比本地 vs 远端） | P0 | L | 5.1, Phase 4 | 支持语法高亮的 diff 展示 |
| 5.4 | 实现选择性同步交互（checkbox 选择要 push/pull 的项） | P0 | M | 5.2 | 用户可勾选/取消勾选 |
| 5.5 | 实现冲突解决交互（展示冲突、选择保留本地/远端/手动合并） | P0 | L | 5.3, 4.5 | 冲突能通过 TUI 解决 |
| 5.6 | 实现 `sync-devices manage` 命令（进入 TUI） | P0 | S | 5.2 | 命令正确启动 TUI |
| 5.7 | 实现设备管理视图（查看已登录设备、管理设备名称） | P1 | M | 5.1 | 能查看和命名设备 |

---

## Phase 6: 质量保障与分发

测试、文档、CI/CD、发布。

| # | 任务 | 优先级 | 工作量 | 依赖 | 验收标准 |
|---|------|--------|--------|------|----------|
| 6.1 | 为核心模块编写单元测试（Adapter, Sanitizer, Sync Engine） | P0 | L | Phase 1, 4 | 测试覆盖核心路径 |
| 6.2 | 编写集成测试（mock CF Workers 端到端测试） | P1 | M | Phase 4, 3 | push/pull 流程测试通过 |
| 6.3 | 编写 README.md（安装说明、使用指南、架构说明） | P0 | M | Phase 5 | 新用户能按文档完成安装和首次同步 |
| 6.4 | 配置 GitHub Actions CI（build + test, 多平台） | P1 | M | 6.1 | PR 自动跑 CI |
| 6.5 | 配置 Release workflow（自动构建多平台二进制、发布 GitHub Release） | P1 | M | 6.4 | tag 后自动发布 |
| 6.6 | 发布到 crates.io | P2 | S | 6.5 | `cargo install sync-devices` 可用 |
| 6.7 | 编写 Homebrew formula | P2 | S | 6.5 | `brew install sync-devices` 可用 |
