# Phase 4: 同步引擎

## 任务清单

- [x] 4.1 实现本地 manifest 生成（扫描 → hash → manifest）
  - 验收：生成的 manifest 准确反映本地配置状态 ✓（已新增本地 manifest 构造入口，`status` 命令会输出设备标识和稳定排序后的本地 manifest 项）
- [x] 4.2 实现 remote manifest 拉取与本地 manifest 对比
  - 验收：正确识别 new/modified/deleted/unchanged 状态 ✓（已实现本地/远端 manifest diff 与摘要统计，`status` 命令可输出 Local only / Remote only / Modified / Unchanged 汇总）
- [ ] 4.3 实现 push 逻辑（上传新增/修改的配置）
  - 验收：增量推送，只上传有变化的项
- [ ] 4.4 实现 pull 逻辑（下载并应用配置，带备份）
  - 验收：应用前备份原文件，应用后验证 hash
- [ ] 4.5 实现冲突检测（本地和远端都有修改）
  - 验收：正确识别冲突项
- [ ] 4.6 实现 `sync-devices push` 命令
  - 验收：命令行交互完整
- [ ] 4.7 实现 `sync-devices pull` 命令
  - 验收：命令行交互完整
- [ ] 4.8 实现 `sync-devices status` 命令
  - 验收：清晰展示差异摘要

## Notes

- 2026-03-22：本地 manifest 生成已落在现有 `adapter` + `model` 模块中，没有额外拆新模块。当前由 `adapter::scan_local_manifest()` 负责扫描本地配置并生成与远端结构对齐的 `SyncManifest`。
- 2026-03-22：本地 manifest 使用 `COMPUTERNAME` / `HOSTNAME` 作为设备标识来源，`generated_at` 使用当前 Unix 秒级时间戳。
- 2026-03-22：manifest 条目按 `tool -> category -> rel_path` 做稳定排序，避免文件系统遍历顺序波动影响后续 diff 结果。
- 2026-03-22：已新增 `diff_manifests()` 和 `summarize_manifest_diff()`，当前以 `tool + category + rel_path` 作为条目身份键；若 `content_hash` 或 `is_device_specific` 不同，则判定为 `Modified`。
- 2026-03-22：当前 `status` 命令在已登录时会同时展示本地 manifest、远端 manifest 元数据和 diff 摘要，作为 Phase 4.2 的可验证入口。
