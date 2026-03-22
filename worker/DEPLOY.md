# Worker 部署说明

本目录的 Cloudflare Worker 依赖一个名为 `SYNC_CONFIGS` 的 KV namespace，以及一组运行时 secrets。

## 前置条件

需要先完成以下准备：

1. 安装依赖：`npm install`
2. 登录 Cloudflare：`npx wrangler login`
3. 准备 GitHub OAuth App 的 `client_id`

## 初始化 KV namespace

先创建生产 namespace：

```bash
npm run kv:create
```

再创建 preview namespace：

```bash
npm run kv:create:preview
```

这两个命令都会调用 Wrangler 的 `--update-config`，自动把真实 namespace id 写回 `wrangler.jsonc`。

## 配置运行时 secrets

至少需要写入下面两个 secrets：

```bash
npx wrangler secret put GITHUB_CLIENT_ID
npx wrangler secret put JWT_SECRET
```

可选 secrets：

```bash
npx wrangler secret put GITHUB_DEVICE_SCOPE
npx wrangler secret put JWT_ISSUER
npx wrangler secret put JWT_TTL_SECONDS
```

本地开发可以参考 `.dev.vars.example`。

## 部署前检查

在执行部署前，可以先跑：

```bash
npm run deploy:check
```

如果 `wrangler.jsonc` 里仍然保留占位 namespace id，这个命令会直接失败，避免误部署。

## 预演与正式部署

先执行 dry run：

```bash
npm run deploy:dry-run
```

确认无误后再正式部署：

```bash
npm run deploy
```
