# Worker 手动部署说明

> 推荐使用 `sync-devices setup` 自动部署。本文档仅供手动部署或调试参考。

## 自动部署 (推荐)

```bash
sync-devices login    # 输入你的 CF API Token
sync-devices setup    # 自动创建 KV + 部署 Worker
```

## 手动部署

如果需要手动管理 Worker（例如调试或自定义配置），按以下步骤操作。

### 前置条件

1. 安装依赖：`npm install`
2. 登录 Cloudflare：`npx wrangler login`

### 初始化 KV namespace

```bash
npm run kv:create
npm run kv:create:preview
```

这两个命令会调用 Wrangler 的 `--update-config`，自动把 namespace id 写回 `wrangler.jsonc`。

### 本地开发

```bash
npm run dev
```

### 构建 JS bundle

```bash
npm run build
```

输出文件在 `dist/worker.js`，这个文件会被 CLI 嵌入到编译产物中。

### 部署

```bash
npm run deploy
```

Worker 使用 CF API Token 认证（`Authorization: Bearer <token>`），不需要配置任何 secrets。KV namespace `SYNC_CONFIGS` 绑定在 `wrangler.jsonc` 中。
