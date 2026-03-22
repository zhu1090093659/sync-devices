# Risk Assessment

## 技术风险

### 高风险

**R1: 敏感信息泄露**
- 描述：配置文件中嵌入了 API Key、Token 等敏感信息（如 `settings.json` 的 env 字段、`config.toml` 的 `shell_environment_policy.set`）。如果未做脱敏处理直接同步到云端，会造成凭据泄露。
- 缓解：实现一个 `Sanitizer` 模块，在上传前扫描并替换/过滤敏感信息。提供白名单/黑名单机制让用户自定义哪些字段不同步。
- 严重性：**Critical**

**R2: 配置格式多样性**
- 描述：三款工具使用了 JSON、TOML、Markdown、自定义格式（`.rules`）等不同格式。合并策略需要理解每种格式的语义结构。
- 缓解：对于结构化格式（JSON/TOML），实现语义级别的 diff/merge。对于文本格式（Markdown/rules），使用行级 diff。
- 严重性：**High**

**R3: 跨平台路径差异**
- 描述：Windows 使用 `\` 和 `C:\Users\`，macOS/Linux 使用 `/` 和 `/Users/` 或 `/home/`。配置中大量引用了本地路径（项目路径、工具路径、扩展路径）。
- 缓解：识别并标记包含路径的配置项为「设备专属」，同步时提供路径映射功能或提示用户手动调整。
- 严重性：**High**

### 中风险

**R4: GitHub OAuth Device Flow 实现**
- 描述：CLI 环境下无法直接弹出浏览器回调，需要使用 Device Flow（用户手动访问 URL 并输入 code）。实现需要处理轮询、超时、token 刷新等。
- 缓解：参考 GitHub 官方文档实现标准 Device Flow，Cloudflare Workers 端处理 token 交换。
- 严重性：**Medium**

**R5: Cloudflare KV 限制**
- 描述：KV 单个 value 最大 25MB，但免费套餐每日读取 100,000 次、写入 1,000 次。对于活跃用户可能接近限制。
- 缓解：实现增量同步（只同步有变化的配置项）、合理的缓存策略、批量读写。使用 content hash 避免不必要的写入。
- 严重性：**Medium**

**R6: 并发冲突**
- 描述：用户可能在多台设备上同时修改配置并推送，导致写入冲突。
- 缓解：TUI 交互式冲突解决是核心设计的一部分。使用乐观锁（基于 content hash）检测冲突，冲突时进入 TUI 让用户手动选择。
- 严重性：**Medium**

### 低风险

**R7: TUI 跨平台兼容性**
- 描述：不同终端模拟器对 ANSI 转义码的支持程度不同，特别是 Windows Terminal vs CMD vs PowerShell。
- 缓解：使用 `crossterm` 作为终端后端（已有良好的 Windows 支持），测试主流终端。
- 严重性：**Low**

**R8: 工具配置结构变更**
- 描述：Claude Code、Codex、Cursor 都在快速迭代，配置文件结构可能随版本更新发生变化。
- 缓解：Config Adapter 模式隔离每个工具的解析逻辑，通过版本号感知配置格式变化。保持 Adapter 实现简洁，便于后续适配新版本。
- 严重性：**Low**

---

## 复杂度热点

| 模块 | 复杂度 | 原因 |
|------|--------|------|
| 敏感信息扫描器 | **高** | 需要识别多种格式中的凭据模式，且不能误报 |
| 结构化配置合并 | **高** | JSON/TOML 的语义级 merge 需要处理嵌套结构和数组 |
| TUI 交互界面 | **高** | Diff 展示、选择性同步、冲突解决的交互设计 |
| GitHub OAuth Device Flow | **中** | 标准流程但涉及多步骤交互和 token 管理 |
| Cloudflare Workers API | **中** | REST API 设计、KV 操作、JWT 验证 |
| Config Adapter (各工具) | **低** | 主要是文件读写和路径解析，逻辑直接 |

---

## 依赖外部因素

| 因素 | 影响 | 应对 |
|------|------|------|
| GitHub OAuth App 注册 | 需要创建 GitHub OAuth App 获取 Client ID | 在 README 中提供注册指引，或预注册一个官方 App |
| Cloudflare 账户 | 需要 CF 账户部署 Workers 和 KV | 提供一键部署脚本，或维护一个公共实例供社区使用 |
| 各工具版本更新 | 配置结构可能变化 | Adapter 模式 + 版本检测 |
