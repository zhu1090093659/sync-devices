# Dependency Graph

## Phase 间依赖关系

```mermaid
graph TD
    P1["Phase 1: 项目骨架与核心数据模型"]
    P2["Phase 2: 后端服务 (CF Workers)"]
    P3["Phase 3: 认证与传输层"]
    P4["Phase 4: 同步引擎"]
    P5["Phase 5: TUI 交互界面"]
    P6["Phase 6: 质量保障与分发"]

    P1 --> P4
    P2 --> P3
    P3 --> P4
    P1 --> P5
    P4 --> P5
    P4 --> P6
    P5 --> P6

    style P1 fill:#4CAF50,color:#fff
    style P2 fill:#2196F3,color:#fff
    style P3 fill:#FF9800,color:#fff
    style P4 fill:#9C27B0,color:#fff
    style P5 fill:#F44336,color:#fff
    style P6 fill:#607D8B,color:#fff
```

## 关键路径

项目的关键路径为：

```
Phase 1 → Phase 4 → Phase 5 → Phase 6
```

Phase 2 和 Phase 3 可以与 Phase 1 **并行开发**，因为它们之间没有直接依赖。

## 任务级别依赖关系

```mermaid
graph LR
    subgraph Phase1["Phase 1: 骨架"]
        T11["1.1 项目初始化"] --> T12["1.2 数据模型"]
        T12 --> T13["1.3 Adapter trait"]
        T13 --> T14["1.4 Claude Adapter"]
        T13 --> T15["1.5 Codex Adapter"]
        T13 --> T16["1.6 Cursor Adapter"]
        T13 --> T17["1.7 Agents Adapter"]
        T12 --> T18["1.8 Sanitizer"]
    end

    subgraph Phase2["Phase 2: 后端"]
        T21["2.1 Workers 初始化"] --> T22["2.2 OAuth 端点"]
        T22 --> T23["2.3 JWT 中间件"]
        T23 --> T24["2.4 上传 API"]
        T23 --> T25["2.5 拉取 API"]
        T23 --> T26["2.6 删除 API"]
        T24 --> T27["2.7 Manifest API"]
    end

    subgraph Phase3["Phase 3: 认证"]
        T22 --> T31["3.1 OAuth 客户端"]
        T31 --> T32["3.2 Token 存储"]
        T31 --> T33["3.3 HTTP Transport"]
        T32 --> T34["3.4 login 命令"]
        T32 --> T35["3.5 logout 命令"]
    end

    subgraph Phase4["Phase 4: 同步引擎"]
        T14 --> T41["4.1 本地 Manifest"]
        T15 --> T41
        T16 --> T41
        T33 --> T42["4.2 Manifest 对比"]
        T41 --> T42
        T42 --> T43["4.3 Push 逻辑"]
        T42 --> T44["4.4 Pull 逻辑"]
        T42 --> T45["4.5 冲突检测"]
        T43 --> T46["4.6 push 命令"]
        T44 --> T47["4.7 pull 命令"]
        T42 --> T48["4.8 status 命令"]
    end

    subgraph Phase5["Phase 5: TUI"]
        T51["5.1 TUI 骨架"] --> T52["5.2 浏览视图"]
        T51 --> T53["5.3 Diff 视图"]
        T52 --> T54["5.4 选择性同步"]
        T53 --> T55["5.5 冲突解决"]
        T52 --> T56["5.6 manage 命令"]
        T51 --> T57["5.7 设备管理"]
    end
```

## 并行开发机会

| 可并行组合 | 说明 |
|------------|------|
| Phase 1 + Phase 2 | Rust 端骨架与 CF Workers 后端可完全并行 |
| T1.4 + T1.5 + T1.6 + T1.7 | 四个 Adapter 相互独立 |
| T2.4 + T2.5 + T2.6 | 三个 CRUD 端点相互独立 |
| T4.3 + T4.4 + T4.5 | Push/Pull/冲突检测基于相同的 diff 结果，可并行 |
| T5.2 + T5.3 | 浏览视图和 Diff 视图可并行开发 |
