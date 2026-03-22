# Phase 6: 质量保障与分发

## 任务清单

- [ ] 6.1 为核心模块编写单元测试（Adapter, Sanitizer, Sync Engine）
  - 验收：测试覆盖核心路径
- [ ] 6.2 编写集成测试（mock CF Workers 端到端测试）
  - 验收：push/pull 流程测试通过
- [ ] 6.3 编写 README.md（安装说明、使用指南、架构说明）
  - 验收：新用户能按文档完成安装和首次同步
- [ ] 6.4 配置 GitHub Actions CI（build + test, 多平台）
  - 验收：PR 自动跑 CI
- [ ] 6.5 配置 Release workflow（自动构建多平台二进制、发布 GitHub Release）
  - 验收：tag 后自动发布
- [ ] 6.6 发布到 crates.io
  - 验收：`cargo install sync-devices` 可用
- [ ] 6.7 编写 Homebrew formula
  - 验收：`brew install sync-devices` 可用

## Notes

（开发过程中的决策和记录）
