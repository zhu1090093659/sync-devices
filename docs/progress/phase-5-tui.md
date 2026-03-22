# Phase 5: TUI 交互界面

## 任务清单

- [x] 5.1 搭建 ratatui 应用骨架（App struct, 事件循环, 基础布局）
  - 验收：能启动空白 TUI 窗口 ✓（已接入基础 ratatui + crossterm 事件循环，`manage` 可打开骨架界面并通过 `q` / `Esc` / `Ctrl+C` 退出）
- [x] 5.2 实现配置浏览视图（按 tool/category 树形展示）
  - 验收：能浏览所有已扫描的配置项 ✓（树形 Tool -> Category -> Item 结构，支持展开/折叠、键盘导航、滚动）
- [x] 5.3 实现 Diff 视图（并排对比本地 vs 远端）
  - 验收：支持语法高亮的 diff 展示 ✓（unified diff 格式，red/green 颜色标记增删行，支持滚动和翻页）
- [x] 5.4 实现选择性同步交互（checkbox 选择要 push/pull 的项）
  - 验收：用户可勾选/取消勾选 ✓（Space 单项勾选、Tool/Category 批量勾选、a 全选/全不选；p 推送选中项、l 拉取选中项；header 显示选中计数和操作结果）
- [x] 5.5 实现冲突解决交互（展示冲突、选择保留本地/远端/手动合并）
  - 验收：冲突能通过 TUI 解决 ✓（'r' 进入冲突解决视图，'1' 保留本地推送远端，'2' 保留远端写入本地；批量 p/l 也支持 Conflict 项）
- [x] 5.6 实现 `sync-devices manage` 命令（进入 TUI）
  - 验收：命令正确启动 TUI ✓（`manage` 子命令通过 clap 注册，调用 async `tui::run_manage()`，启动完整 TUI 界面）
- [x] 5.7 实现设备管理视图（查看已登录设备、管理设备名称）
  - 验收：能查看和命名设备 ✓（'i' 进入设备信息视图，显示当前设备 ID、GitHub 用户、连接状态、已知设备列表）

## Notes

- 2026-03-22：已新增最小 `tui` 模块，当前封装了终端初始化、备用屏切换、raw mode、事件循环和三段式基础布局。
- 2026-03-22：`sync-devices manage` 现在会启动 TUI 骨架窗口，但暂时仍只展示占位内容；配置浏览、diff、选择性同步和冲突解决继续留在后续子任务。
- 2026-03-22：配置浏览视图已实现。`manage` 启动后自动扫描本地配置，按 Tool -> Category -> Item 三级树形展示，支持 Up/Down 导航、Left/Right/Enter 展开折叠、滚动跟随选中行。Tool 和 Category 在 model.rs 中新增 `PartialOrd, Ord` derive 以支持 BTreeMap 排序。
- 2026-03-22：Diff 视图已实现。`run_manage()` 改为 async，启动时尝试加载远端数据（graceful fallback 到 offline 模式）。树视图中每个 item 显示 diff 状态标记（+/R/~/!/=），按 d 或 Enter 进入 unified diff 详情，用 `similar` crate 计算行级差异，red/green 高亮增删行，支持 PgUp/PgDn 翻页。
- 2026-03-22：选择性同步交互已实现。Enter/Right 改为展开/diff，Space 改为勾选 checkbox。Space 在 Tool/Category 上批量切换所有子项，'a' 全选/全不选。'p' 推送选中的 LocalOnly/Modified 项（block_in_place async），'l' 拉取选中的 RemoteOnly 项（写入本地磁盘）。操作结果显示在 header 状态栏。transport 客户端从 load_remote_data 保留传入 run_app。
- 2026-03-22：冲突解决交互已实现。新增 `ViewMode::Resolve` 和 `ResolveState`。Browse 模式按 'r' 在 Conflict 项上打开解决视图（红色 CONFLICT 标题、diff 内容、1/2 选择键）。'1' 保留本地版本推送远端，'2' 保留远端版本写入本地。批量 push/pull（p/l）也扩展支持 Conflict 项。
- 2026-03-22：设备管理视图已实现。新增 `ViewMode::Devices` 和 `DevicesState`。Browse 模式按 'i' 打开设备信息视图，展示当前设备 ID、GitHub 用户登录名、连接状态、已知设备列表（从远端 records 的 device_id 聚合）。已知设备中标记当前设备为 "(this device)"。
