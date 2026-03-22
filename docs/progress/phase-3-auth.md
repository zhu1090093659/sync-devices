# Phase 3: 认证与传输层

## 任务清单

- [x] 3.1 实现 GitHub OAuth Device Flow 客户端
  - 验收：CLI 执行 login 后获得 access token ✓（已对接 Worker 的 device code / token 端点，实测 GitHub 授权后成功换回 session token）
- [x] 3.2 实现 token 本地安全存储（keyring 或加密文件）
  - 验收：token 不以明文存储 ✓（已切换到平台原生 keyring 后端，登录成功后会话写入系统凭据存储，不再明文打印）
- [x] 3.3 实现 HTTP Transport 层（封装 reqwest，处理认证头、重试、错误）
  - 验收：能调通后端所有 API ✓（已实测调用 `/api/session`、`/api/manifest`、`/api/configs`，live roundtrip 上传/删除配置通过）
- [x] 3.4 实现 `sync-devices login` 命令
  - 验收：用户能完成登录流程 ✓（已实测输出 GitHub 验证地址和用户码，授权后成功写入系统 keyring）
- [x] 3.5 实现 `sync-devices logout` 命令
  - 验收：清除本地 token ✓（已实测跨进程读取并删除系统 keyring 中的已登录会话）

## Notes

- 2026-03-22：已新增 Rust 侧 Device Flow 客户端模块，当前 `sync-devices login` 会对接 Worker 的 `/api/auth/device/code` 和 `/api/auth/device/token`，打印验证地址、用户码，并轮询等待 session token。
- 2026-03-22：本地 `cargo check` 已通过；在补齐线上 `GITHUB_CLIENT_ID` secret 后，`cargo run -- login` 已实测完成 GitHub Device Flow，并成功获得 Worker 签发的 session token。
- 2026-03-22：补充了系统 keyring 会话存储，并切换到平台原生持久后端。之前反复重新授权的原因是默认 keyring 特性没有启用原生存储，导致新进程无法读回已保存会话；修正后已验证 `login -> logout` 可跨进程工作。
- 2026-03-22：已完成 HTTP Transport 层，当前统一封装了 session、manifest、config list、upload、delete 请求，自动附带 Bearer token，并对 `429`/`5xx` 做一次轻量重试。
- 2026-03-22：修复了配置删除请求对 config id 的双重编码问题。此前 `DELETE /api/configs/:id` 在 `rel_path` 含 `/` 时会把 `%2F` 继续编码成 `%252F`，导致后端删不到对应 KV key。
- 2026-03-22：补充了 live roundtrip 验证。由于 Cloudflare KV 的 list/read-after-write 可见性不是强一致，live test 改为验证各 API 调用与上传/删除闭环成功，而不再要求上传后列表立即反映新增项。
