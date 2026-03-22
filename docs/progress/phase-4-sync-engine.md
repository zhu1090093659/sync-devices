# Phase 4: 同步引擎

## 任务清单

- [x] 4.1 实现本地 manifest 生成（扫描 → hash → manifest）
  - 验收：生成的 manifest 准确反映本地配置状态 ✓（已新增本地 manifest 构造入口，`status` 命令会输出设备标识和稳定排序后的本地 manifest 项）
- [x] 4.2 实现 remote manifest 拉取与本地 manifest 对比
  - 验收：正确识别 new/modified/deleted/unchanged 状态 ✓（已实现本地/远端 manifest diff 与摘要统计，`status` 命令可输出 Local only / Remote only / Modified / Unchanged 汇总）
- [x] 4.3 实现 push 逻辑（上传新增/修改的配置）
  - 验收：增量推送，只上传有变化的项 ✓（已基于 manifest diff 仅上传 `LocalOnly` / `Modified` 项，当前远端对齐后再次执行 `push` 会返回 `No local changes to push.`）
- [x] 4.4 实现 pull 逻辑（下载并应用配置，带备份）
  - 验收：应用前备份原文件，应用后验证 hash ✓（当前 `pull` 会应用 `RemoteOnly` 项；覆盖前创建 `.sync-devices.bak.<timestamp>` 备份，写入后重新计算 hash 校验）
- [x] 4.5 实现冲突检测（本地和远端都有修改）
  - 验收：正确识别冲突项 ✓（当前 manifest 条目已携带 `device_id`；当本地与远端内容不同，且远端最后写入设备不是当前设备时，保守标记为 `Conflict`，`push` / `pull` 不会自动覆盖）
- [x] 4.6 实现 `sync-devices push` 命令
  - 验收：命令行交互完整 ✓（已输出待上传的新建/修改计数和最终上传结果；当前无变更时会直接提示无须推送）
- [x] 4.7 实现 `sync-devices pull` 命令
  - 验收：命令行交互完整 ✓（当前会先输出本地快照概况，再拉取远端；无远端变更、存在冲突/修改、未登录等分支均有明确 CLI 提示）
- [x] 4.8 实现 `sync-devices status` 命令
  - 验收：清晰展示差异摘要 ✓（已展示本地可同步项、远端会话信息以及 manifest diff 摘要）

## Notes

- 2026-03-22：本地 manifest 生成已落在现有 `adapter` + `model` 模块中，没有额外拆新模块。当前由 `adapter::scan_local_manifest()` 负责扫描本地配置并生成与远端结构对齐的 `SyncManifest`。
- 2026-03-22：本地 manifest 使用 `COMPUTERNAME` / `HOSTNAME` 作为设备标识来源，`generated_at` 使用当前 Unix 秒级时间戳。
- 2026-03-22：manifest 条目按 `tool -> category -> rel_path` 做稳定排序，避免文件系统遍历顺序波动影响后续 diff 结果。
- 2026-03-22：已新增 `diff_manifests()` 和 `summarize_manifest_diff()`，当前以 `tool + category + rel_path` 作为条目身份键；若 `content_hash` 或 `is_device_specific` 不同，则判定为 `Modified`。
- 2026-03-22：当前 `status` 命令在已登录时会同时展示本地 manifest、远端 manifest 元数据和 diff 摘要，作为 Phase 4.2 的可验证入口。
- 2026-03-22：新增了本地 sync snapshot 预处理，上传前会对配置内容做统一脱敏，并把设备标识写入待同步项；本地 manifest 也基于这份脱敏后的 snapshot 生成，以避免 push 后因 hash 不一致反复出现假修改。
- 2026-03-22：新增 `build_push_plan()`，当前只会选择 `LocalOnly` 和 `Modified` 条目进入上传队列，`RemoteOnly` / `Unchanged` 不会被重复推送。
- 2026-03-22：已补充受控 live push smoke，使用单条临时 shared-agent skill 验证上传响应包含脱敏后的内容，并在测试结束时删除远端记录。
- 2026-03-22：已补充 `pull` 安全落盘逻辑；当前只自动应用 `RemoteOnly` 项，`Modified` 先跳过等待 4.5 冲突检测。覆盖已有文件前会先备份，落盘后重新计算内容 hash 校验。
- 2026-03-22：transport 现已为 API 请求设置全局连接/请求超时，避免 `pull` / `status` 在异常网络下无限挂起。
- 2026-03-22：remote manifest 条目已补充 `device_id`，当前冲突检测采用保守规则：若本地和远端内容不同，且远端最后写入设备不是当前设备，则归类为 `Conflict`；`push` 会跳过这些项并提示冲突计数，`pull` 也不会自动覆盖。
- 2026-03-22：`pull` 命令交互已补齐，当前会先打印本地可同步项数量和设备标识；未登录时不再直接抛错误，而是明确提示先执行 `sync-devices login`。
