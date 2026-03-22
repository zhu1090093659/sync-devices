# Phase 2: 后端服务 (Cloudflare Workers + KV)

## 任务清单

- [x] 2.1 搭建 CF Workers 项目结构（wrangler init）
  - 验收：`wrangler dev` 本地运行 ✓（Hono Worker 骨架已创建，`npm run typecheck` 和 `npm run dev` 已验证）
- [x] 2.2 实现 GitHub OAuth Device Flow 后端（token 交换端点）
  - 验收：能完成 device code → access token 交换 ✓（已实现 `/api/auth/device/code` 和 `/api/auth/device/token`，本地路由与错误语义已验证）
- [x] 2.3 实现 JWT 认证中间件（验证请求身份）
  - 验收：未授权请求返回 401 ✓（`/api/session` 已接入 JWT 中间件，本地验证无 token 为 401、有效 token 为 200）
- [x] 2.4 实现配置上传 API（PUT /api/configs/:tool/:category/:path）
  - 验收：配置正确存入 KV ✓（`PUT /api/configs/:tool/:category/*` 已接入 JWT 鉴权，写入后会立即从 KV 回读并返回 `201`）
- [x] 2.5 实现配置拉取 API（GET /api/configs）
  - 验收：能按 tool/category 过滤返回配置 ✓（`GET /api/configs` 已支持 `tool` / `category` 查询参数，本地验证了全量和过滤返回）
- [x] 2.6 实现配置删除 API（DELETE /api/configs/:id）
  - 验收：配置从 KV 中删除 ✓（已复用现有 `StoredConfigRecord.id` 和 KV key 规则，本地验证删除后查询为空、重复删除返回 404）
- [x] 2.7 实现配置元数据 API（GET /api/manifest）
  - 验收：返回所有配置项的 hash 和时间戳 ✓（已返回与 Rust `SyncManifest` 对齐的 `device_id`、`generated_at`、`items`，本地验证仅包含元数据字段）
- [x] 2.8 部署脚本和 KV namespace 配置
  - 验收：`wrangler deploy` 成功 ✓（已创建 `SYNC_CONFIGS` 的生产与 preview namespace，本地 `deploy:check` / `deploy:dry-run` 通过，正式部署到 `workers.dev` 成功）

## Notes

- 2026-03-22：新增 `worker/` 子项目，使用 Hono + Wrangler 4，当前提供 `/` 和 `/healthz` 基础路由作为本地运行验证入口。
- 2026-03-22：新增 GitHub Device Flow 后端路由，Worker 通过 `GITHUB_CLIENT_ID` 和可选 `GITHUB_DEVICE_SCOPE` 转发 GitHub OAuth 请求；本地已验证缺失配置和非法请求体的返回语义。
- 2026-03-22：新增内部 JWT 会话签发与校验逻辑。`/api/auth/device/token` 在拿到 GitHub access token 后会拉取用户资料并签发内部 session token；`/api/session` 作为受保护探针用于验证鉴权链路。
- 2026-03-22：新增配置上传 API，当前使用 `SYNC_CONFIGS` KV 绑定按 `user -> tool -> category -> rel_path` 维度存储完整配置记录；本地已验证未授权为 401、授权写入后可从 KV 读回记录。
- 2026-03-22：新增配置拉取 API，复用了上传侧的 KV key 规则；当前支持按 `tool`、`category` 查询参数过滤返回完整配置记录列表。
- 2026-03-22：新增配置删除 API，当前通过 `DELETE /api/configs/:id` 按登录用户作用域删除记录；本地已验证成功删除后列表为空，重复删除返回 `404 config_not_found`。
- 2026-03-22：新增配置元数据 API，当前通过 `GET /api/manifest` 返回远端 manifest 快照；顶层 `device_id` 使用稳定的远端来源标识，`items` 仅包含 `tool`、`category`、`rel_path`、`content_hash`、`last_modified`、`is_device_specific`。
- 2026-03-22：补充了 Worker 部署脚本、KV namespace 初始化命令和部署前校验；已创建 `SYNC_CONFIGS` 的生产 namespace `f7b5f3df048a4b62b6bf2d8bdd66464f` 和 preview namespace `276f5be9896a4dc18430f9d6e3c535be`，并已成功部署到 `https://sync-devices-worker.1090093659.workers.dev`。
- 2026-03-22：线上 Worker 当前已配置 `JWT_SECRET`，根路由返回正常；认证入口仍缺少 `GITHUB_CLIENT_ID`，调用 `/api/auth/device/code` 会返回 `Missing GITHUB_CLIENT_ID binding.`，后续需要补齐该 secret 才能做端到端登录验证。
