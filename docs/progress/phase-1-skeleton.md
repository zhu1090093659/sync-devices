# Phase 1: 项目骨架与核心数据模型

## 任务清单

- [x] 1.1 初始化 Rust 项目（cargo init, 依赖声明, 目录结构）
  - 验收：`cargo build` 通过 ✓
- [x] 1.2 定义核心数据模型（ConfigItem, Tool, Category, SyncManifest）
  - 验收：模型可序列化/反序列化 ✓
- [x] 1.3 实现 Config Adapter trait 和三个工具的 Adapter 骨架
  - 验收：每个 Adapter 能扫描并列出可同步项 ✓
- [x] 1.4 实现 Claude Code Adapter（扫描 ~/.claude/）
  - 验收：正确识别 settings.json, CLAUDE.md, commands/, skills/, plugins 配置 ✓
- [x] 1.5 实现 Codex Adapter（扫描 ~/.codex/）
  - 验收：正确识别 config.toml, AGENTS.md, rules/, skills/ ✓
- [x] 1.6 实现 Cursor Adapter（扫描 ~/.cursor/）
  - 验收：正确识别 mcp.json, commands/, rules/ ✓
- [x] 1.7 实现 Shared Agents Adapter（扫描 ~/.agents/）
  - 验收：正确识别 skills/ ✓
- [x] 1.8 实现敏感信息扫描器（Sanitizer）
  - 验收：能检测并替换 API Key、Token 等敏感模式 ✓（4 个单元测试通过）

## Notes

- 实际扫描发现 243 个可同步项，覆盖四款工具
- Claude Code skills 目录下有大量第三方插件安装的 skills，与 ~/.agents/skills 有重复
- Codex config.toml 中的 shell_environment_policy.set 包含大量敏感环境变量（API Key 等）
- Sanitizer 使用 LazyLock 实现正则编译缓存，支持 API Key、GitHub PAT、Bearer Token、Base64 等模式
