# MASTER: sync_devices

## 任务定义

开发一个基于 Rust 的跨平台 CLI 工具（含 TUI），通过 GitHub OAuth 认证和 Cloudflare Workers + KV 后端，在多台设备间同步和管理 Claude Code、Codex、Cursor 的配置信息。

## 文档索引

### 分析文档
- [项目概览](../analysis/project-overview.md)
- [模块清单](../analysis/module-inventory.md)
- [风险评估](../analysis/risk-assessment.md)

### 计划文档
- [任务分解](../plan/task-breakdown.md)
- [依赖图](../plan/dependency-graph.md)
- [里程碑](../plan/milestones.md)

## 各阶段进度总览

| 阶段 | 状态 | 进度 | 详情 |
|------|------|------|------|
| Phase 1: 项目骨架与核心数据模型 | 已完成 | 8/8 | [详情](./phase-1-skeleton.md) |
| Phase 2: 后端服务 (CF Workers) | 已完成 | 8/8 | [详情](./phase-2-backend.md) |
| Phase 3: 认证与传输层 | 已完成 | 5/5 | [详情](./phase-3-auth.md) |
| Phase 4: 同步引擎 | 进行中 | 2/8 | [详情](./phase-4-sync-engine.md) |
| Phase 5: TUI 交互界面 | 待开始 | 0/7 | [详情](./phase-5-tui.md) |
| Phase 6: 质量保障与分发 | 待开始 | 0/7 | [详情](./phase-6-release.md) |

**总进度：23/43 任务**

## Current Status

Phase 1、Phase 2 和 Phase 3 已全部完成，Phase 4 已完成 4.1 和 4.2。本地配置现在可以生成稳定排序的 `SyncManifest`，并且已能与远端 manifest 做差异比较；当前 `status` 命令会输出设备标识、远端会话信息以及 diff 摘要，为后续 push/pull 增量同步提供基础。

## Next Steps

1. Phase 4.3: 实现 push 逻辑（仅上传新增或修改项）
2. Phase 4.4: 实现 pull 逻辑（下载并应用配置，带备份）
3. Phase 4.5: 实现冲突检测（本地和远端都有修改）
