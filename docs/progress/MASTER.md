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
| Phase 4: 同步引擎 | 已完成 | 8/8 | [详情](./phase-4-sync-engine.md) |
| Phase 5: TUI 交互界面 | 已完成 | 7/7 | [详情](./phase-5-tui.md) |
| Phase 6: 质量保障与分发 | 待开始 | 0/7 | [详情](./phase-6-release.md) |

**总进度：36/43 任务**

## Current Status

Phase 1-5 已全部完成。TUI 具备完整功能：配置浏览、diff 查看、checkbox 选择、push/pull 执行、冲突解决、设备管理。进入 Phase 6 质量保障与分发。

## Next Steps

1. Phase 6: 质量保障与分发
