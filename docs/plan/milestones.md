# Milestones

## M1: 能扫描 — "知道有什么"

**完成条件**：Phase 1 全部完成

**标志性能力**：
- 运行 CLI 后能列出三款工具所有可同步的配置项
- 敏感信息能被正确识别和标记
- 输出一份本地配置清单（manifest）

**验证方式**：运行一个临时命令，打印扫描到的所有 ConfigItem 列表。

---

## M2: 能登录 — "连上云端"

**完成条件**：Phase 2 + Phase 3 全部完成

**标志性能力**：
- 用户能通过 `sync-devices login` 完成 GitHub OAuth 登录
- Token 安全存储在本地
- CLI 能与 CF Workers 后端成功通信（认证请求通过）

**验证方式**：`sync-devices login` 后调用一个 health check 端点确认身份。

---

## M3: 能同步 — "推拉自如"

**完成条件**：Phase 4 全部完成

**标志性能力**：
- `sync-devices push` 将本地配置上传到云端
- `sync-devices pull` 从云端下载配置并应用到本地
- `sync-devices status` 展示本地与云端的差异
- 冲突能被检测到

**验证方式**：在同一台设备上模拟修改 → push → 修改 → pull，验证数据一致性。

---

## M4: 能交互 — "好用的 TUI"

**完成条件**：Phase 5 全部完成

**标志性能力**：
- `sync-devices manage` 打开交互式 TUI
- 能浏览、对比、选择性同步配置
- 冲突通过 TUI 交互解决

**验证方式**：完整执行一次 TUI 内的 push/pull 流程。

---

## M5: 能发布 — "社区可用"

**完成条件**：Phase 6 全部完成

**标志性能力**：
- 核心模块有测试覆盖
- GitHub Actions CI 跑通
- 用户可通过 `cargo install` 或 GitHub Release 安装
- README 文档完整

**验证方式**：全新环境下按 README 指引完成安装、登录、首次同步。
