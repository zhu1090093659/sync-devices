# sync-devices 架构转型风险评估

本文档识别从"共享 Worker + GitHub OAuth"向"自部署 Worker + Cloudflare API Token"转型过程中的关键风险，并给出缓解建议。

---

## R1: Cloudflare API 速率限制与 Token 权限

**风险等级：** 高

**描述**

CLI 将通过 Cloudflare REST API 完成 Worker 部署和 KV namespace 创建。CF API 对免费账户有速率限制（通常是 1200 requests/5min），而部署流程涉及多个 API 调用（创建 Worker、上传脚本、绑定 KV、设置 secrets）。如果部署流程实现不当（重试过于激进、缺少缓存），可能触发限流。

更关键的问题是 Token 权限范围。Cloudflare API Token 的权限模型基于 Resources + Permissions 的组合，用户创建 Token 时需要精确选择以下权限：

- **Account / Workers Scripts / Edit** -- 部署和管理 Worker 脚本
- **Account / Workers KV Storage / Edit** -- 创建和操作 KV namespace
- **Account / Account Settings / Read** -- 读取 Account ID（部署时需要）

如果用户授予了过多权限（比如 Zone 级别的 DNS 管理），Token 泄漏的后果将远超项目本身的影响范围。如果授予权限不足，部署过程会报出晦涩的 403 错误。

**缓解措施**

CLI 应在文档和交互流程中明确列出所需的最小权限集，最好提供一个可以直接点击的 Token 创建链接（CF 支持预填充权限的 URL）。在 Token 验证阶段，主动检测权限是否充足，给出明确的缺失权限提示而非裸露的 HTTP 403。

---

## R2: Worker 部署复杂度

**风险等级：** 高

**描述**

让终端用户通过 CLI 完成 Worker 部署是整个转型中最不确定的环节。CF Workers 部署涉及多个步骤：

1. 验证 API Token 有效性
2. 获取 Account ID（一个账户可能有多个 account）
3. 创建 KV namespace
4. 上传 Worker 脚本（CLI 内嵌的 JS 代码）
5. 绑定 KV namespace 到 Worker
6. 设置 Worker 路由或自定义域名

这其中任何一步都可能失败。用户可能已经有同名的 Worker 或 KV namespace，CF 免费计划对 Worker 数量有上限（目前是 100 个），KV 的免费读写配额也有限制。

另一个复杂性来自于 Worker 的版本更新。当 CLI 升级后内嵌的 Worker 代码发生变化时，需要有机制检测并重新部署。

**缓解措施**

将部署流程设计为幂等操作——如果 Worker 和 KV 已存在就跳过创建，只更新脚本。提供清晰的错误信息和恢复指导。考虑增加 `sync-devices deploy --dry-run` 命令让用户预览部署计划。对 Worker 代码嵌入版本号，deploy 时比对远端版本决定是否需要更新。

---

## R3: KV 性能问题（现存缺陷）

**风险等级：** 高（已经在生产环境中出现）

**描述**

当前 `config-store.ts` 中的 `getConfigManifest` 函数存在严重的性能缺陷。它调用 `listConfigRecords`，后者的实现是：先通过 KV `list()` 获取所有 key，然后在 `Promise.all` 中对每个 key 执行 `get()` 读取完整记录。这段代码如下：

```typescript
const batch = await Promise.all(
  result.keys.map((key) => store.get<StoredConfigRecord>(key.name, "json")),
);
```

Cloudflare Workers 的 CPU 时间限制是 10ms（免费计划）或 50ms（付费计划），wall-clock 时间限制是 30 秒。当配置数量超过 50-100 条时，几十个并行 KV 读取会耗尽时间配额，导致 Worker 被强制终止。

这个问题在迁移到自部署模型后依然存在，而且由于免费计划的 KV 读取限额（每天 100,000 次），大量并行读取还会加速配额消耗。

**缓解措施**

必须在架构转型中一并修复此问题。推荐方案有两个层次：

第一层（短期）：限制并行度。将 `Promise.all` 改为分批串行处理，每批不超过 10 个并行请求。这可以避免超时，但仍然需要多次 KV 读取。

第二层（根本）：在 `saveConfigRecord` 时同步更新一个独立的 manifest 缓存 key（例如 `manifest:{owner}`），manifest 端点直接读取这个单一 key 而不是重新组装。这样 manifest 请求只需一次 KV 读取，响应时间从秒级降到毫秒级。代价是写入时多一次 KV 操作，但写入频率远低于读取频率。

---

## R4: 对现有用户的破坏性变更

**风险等级：** 中

**描述**

架构转型将彻底改变认证方式和服务端部署模型，对现有用户构成不兼容变更。具体表现为：

- 现有的 JWT session 将失效，`session_store` 中的旧数据无法被新版本解析
- 存储在共享 Worker KV 中的配置数据无法自动迁移到用户自己的 KV
- `login` 命令的交互流程完全不同（从浏览器授权 GitHub 变为粘贴 CF API Token）

由于项目尚处于早期阶段（版本 0.1.0），用户基数可能很小，但仍然需要提供一个体面的迁移路径。

**缓解措施**

在 CLI 新版本中检测旧格式的 session 数据，清理后给出友好的重新登录提示（而非报错 crash）。如果可能，提供一个 `migrate` 命令从共享 Worker 导出现有配置数据，然后导入到用户的新 Worker。在 CHANGELOG 和 release notes 中明确说明破坏性变更。考虑使用语义化版本升级到 0.2.0 来标记此次变更。

---

## R5: CLI 二进制体积增长

**风险等级：** 低

**描述**

CLI 需要内嵌 Worker 的 JS 源码。当前 Worker 代码总计约 1200 行 TypeScript，但需要经过 wrangler 打包后才能部署，打包产物可能包含 Hono 框架的运行时代码。如果直接内嵌打包后的 JS bundle，可能增加数百 KB 到 1MB 的体积。

此外，如果未来 Worker 代码增长或引入更多依赖，内嵌体积会持续膨胀。

**缓解措施**

首先评估实际的打包体积。Hono 本身非常轻量（核心 < 20KB），加上业务代码，总 bundle 大小应该在 50-100KB 量级。在 Rust 侧使用 `include_str!` 或 `include_bytes!` 宏内嵌，可以考虑 gzip 压缩存储再运行时解压。当前 CLI 二进制大小（纯 Rust + rustls + ratatui）应该在 5-10MB 量级，增加 100KB 影响可以忽略。如果实际体积超出预期，可以改为从 GitHub Release 下载 Worker 代码再部署。

---

## R6: 中国地区网络限制

**风险等级：** 中

**描述**

这是在实际调试中发现的问题。`.workers.dev` 域名在中国大陆的访问不稳定，部分网络环境下需要代理才能连通。这个问题在当前的共享 Worker 模型下已经存在，转型到自部署模型后情况取决于用户的域名配置：

- 如果用户仍然使用默认的 `.workers.dev` 子域名，问题依旧
- 如果用户绑定了自定义域名（通过 CF DNS），则可以绕过此限制

另一个层面的网络问题是 CLI 部署 Worker 时需要访问 `api.cloudflare.com`，这个域名在中国的可访问性同样不够稳定。

**缓解措施**

CLI 支持通过环境变量或配置文件设置 HTTP 代理（reqwest 本身支持 `HTTPS_PROXY` 环境变量）。在文档中建议中国用户绑定自定义域名。部署流程中如果检测到网络超时，给出明确的代理配置提示。transport 模块当前已有重试逻辑（3 次、500ms 间隔），在代理场景下可能需要增加超时阈值。

---

## R7: Token 安全性

**风险等级：** 中

**描述**

Cloudflare API Token 的能力范围比项目实际需要的大得多。即使按照最小权限原则创建，一个具有 Workers Scripts Edit 权限的 Token 也可以管理账户下所有 Worker。如果 Token 从 keyring 泄漏（比如 macOS keychain 被恶意软件读取、Linux 上使用了不安全的 keyring 后端），攻击者可以修改用户的所有 Worker 代码，这比泄漏一个只读的 GitHub OAuth scope 严重得多。

另一个安全考量是 Token 在 CLI 端的传输。当前架构中 CLI 与 Worker 的通信使用 HTTPS + Bearer JWT，JWT 泄漏只能影响 sync-devices 服务。新架构中如果 CLI 直接将 CF API Token 作为 Bearer Token 传给自己的 Worker，而 Worker 代码被篡改，Token 可能被窃取。

**缓解措施**

严格区分两类 Token 的用途。部署阶段使用 CF API Token（高权限，仅在 deploy 时使用），日常同步操作使用 Worker 自己的 auth secret（低权限，可以是部署时随机生成并存入 Worker secrets 的一个简单 token）。这样日常通信中传输的只是一个仅对 sync-devices Worker 有效的 secret，而非 CF API Token 本身。

在 keyring 存储中，考虑将 CF API Token 和日常 sync token 分开存储在不同的 keyring entry 中。sanitizer 模块可以增加对 CF API Token 格式的识别规则（CF Token 通常以固定前缀开头），防止 Token 被意外写入同步的配置文件中。

---

## 风险矩阵总览

| 编号 | 风险项 | 等级 | 影响面 | 紧迫度 |
|------|--------|------|--------|--------|
| R1 | CF API 速率限制与 Token 权限 | 高 | 部署流程 | 设计阶段必须解决 |
| R2 | Worker 部署复杂度 | 高 | 用户体验 | 设计阶段必须解决 |
| R3 | KV 性能缺陷 | 高 | 可用性 | 迁移中必须修复 |
| R4 | 现有用户破坏性变更 | 中 | 兼容性 | 发版前解决 |
| R5 | 二进制体积增长 | 低 | 分发 | 实际评估后决定 |
| R6 | 中国地区网络限制 | 中 | 可用性 | 文档 + 代理支持 |
| R7 | Token 安全性 | 中 | 安全 | 设计阶段必须解决 |

R1、R2、R3 三项构成了转型的核心技术挑战，需要在详细设计阶段给出完整方案后才能进入开发。R7 的 Token 分层策略（部署 Token vs 同步 Token）对整体架构有决定性影响，也需要尽早敲定。R4 和 R6 属于已知问题的应对策略，可以在实现过程中逐步完善。R5 的实际影响很小，不构成阻塞。
