# Module Inventory: 同步目标工具配置结构

## 1. Claude Code (`~/.claude/`)

### 可同步配置

| 文件/目录 | 格式 | 说明 | 优先级 |
|-----------|------|------|--------|
| `settings.json` | JSON | 主配置：权限、模型、插件开关、语言偏好 | P0 |
| `CLAUDE.md` | Markdown | 全局指令/行为规则 | P0 |
| `commands/*.md` | Markdown | 自定义斜杠命令 | P0 |
| `config.json` | JSON | API Key 配置（需脱敏处理） | P1 |
| `plugins/installed_plugins.json` | JSON | 已安装插件清单 | P1 |
| `plugins/known_marketplaces.json` | JSON | 自定义插件市场 | P1 |
| `settings.local.json` | JSON | 本地设置覆盖（需标记为设备专属） | P2 |
| `CLAUDE.local.md` | Markdown | 本地指令（设备专属） | P2 |
| `skills/` | 目录(Markdown) | 用户自定义 Skills | P0 |

### 不可同步（运行时/敏感数据）

| 文件/目录 | 原因 |
|-----------|------|
| `.credentials.json` | 认证凭据，安全敏感 |
| `debug/`, `backups/` | 调试/备份数据，体积大 |
| `sessions/`, `history.jsonl` | 会话历史，设备本地 |
| `plugins/cache/` | 插件二进制缓存，平台相关 |
| `plans/`, `todos/`, `tasks/` | 会话状态，临时数据 |

---

## 2. Codex (`~/.codex/`)

### 可同步配置

| 文件/目录 | 格式 | 说明 | 优先级 |
|-----------|------|------|--------|
| `config.toml` | TOML | 主配置：模型、MCP servers、features、shell 环境等 | P0 |
| `AGENTS.md` | Markdown | 全局 Agent 指令 | P0 |
| `rules/default.rules` | 自定义格式 | 审批规则 | P1 |
| `skills/*/SKILL.md` | Markdown | 用户 Skills（排除 `.system/`） | P0 |

### 不可同步

| 文件/目录 | 原因 |
|-----------|------|
| `auth.json` | 认证凭据 |
| `.codex-global-state.json` | 运行时状态 |
| `history.jsonl`, `session_index.jsonl` | 会话历史 |
| `logs_1.sqlite` | 日志数据库 |
| `.sandbox*/` | 沙箱运行时 |

### 特殊处理

`config.toml` 中包含需要按设备差异化处理的字段：
- `[shell_environment_policy.set]`：包含路径相关的环境变量（如 `CARGO_HOME`、`RUSTUP_HOME`），需要设备专属处理
- `[projects.*]`：项目信任配置，路径为设备本地路径
- `[model_providers.custom]`：可能包含 API Key

---

## 3. Cursor (`~/.cursor/`)

### 可同步配置

| 文件/目录 | 格式 | 说明 | 优先级 |
|-----------|------|------|--------|
| `mcp.json` | JSON | MCP Server 配置 | P0 |
| `commands/*.md` | Markdown | 自定义命令 | P0 |
| `rules/` | 目录(各种格式) | Cursor Rules（如果存在） | P1 |

### 不可同步

| 文件/目录 | 原因 |
|-----------|------|
| `extensions/` | 二进制扩展，平台相关，体积巨大 |
| `ai-tracking/` | 本地追踪数据库 |
| `argv.json` | 运行时参数 |

---

## 4. 跨工具共享目录 (`~/.agents/`)

| 文件/目录 | 格式 | 说明 | 优先级 |
|-----------|------|------|--------|
| `skills/*/` | 目录(Markdown) | 跨工具共享 Skills | P0 |
| `.skill-lock.json` | JSON | Skill 锁定文件 | P1 |

---

## 5. 敏感数据处理策略

所有配置文件在同步前需经过敏感信息扫描：

| 敏感模式 | 处理方式 |
|----------|----------|
| API Key (`sk-*`, `ace_*` 等) | 替换为占位符 `<REDACTED:api_key>` |
| Token (`Bearer *`) | 替换为占位符 |
| 本地文件路径 | 标记为设备专属，同步时提示用户确认 |
| 环境变量中的凭据 | 过滤不同步 |

---

## 数据模型设计

每个可同步项抽象为 `ConfigItem`：

```rust
struct ConfigItem {
    tool: Tool,           // claude_code | codex | cursor | shared_agents
    category: Category,   // settings | instructions | commands | skills | mcp | plugins | rules
    rel_path: String,     // relative path within tool's config dir
    content: String,      // file content
    content_hash: String, // SHA-256 for change detection
    last_modified: u64,   // unix timestamp
    device_id: String,    // originating device identifier
    is_device_specific: bool, // whether this item should be per-device
}
```
