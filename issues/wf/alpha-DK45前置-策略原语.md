# Alpha · DK-45 前置：插件沙箱策略原语（a9b039f）

**实施**: Alpha 2026-10-08 18:40｜**依据**: 用户四案排序第 2 位（插件市场深化）等待期 Alpha 代工
**来源**: DK-43 验收注记——宿主策略设置入口未落地（非阻断缺口）

---

## 一、落地面（traits/plugin_policy.rs，+~90 行）

| 原语 | 语义 |
|---|---|
| `set_plugin_sandbox_policy(kv, publisher, limits)` | 宿主管理面写入——publisher 维度持久化（`settings:plugin_policy:{author}`，与 wifi_only 同 settings 域）|
| `plugin_sandbox_policy(kv, publisher)` | 查询；未配置 `None`（回退缺省安全值）；**坏 JSON 容错 None+warn**（策略损坏不阻断安装）|
| `apply_sandbox_policy(manifest, Option<SandboxLimits>)` | 安装路径覆盖（to_manifest 之后、load 之前）；`None` 不动（保持 listing 缺省）|

空 publisher 拒绝（A04）。测试 5 项：roundtrip/多 publisher 隔离/空 publisher 拒绝/坏 JSON 容错/apply 覆盖与保持。

## 二、与 DK-42/43 的链路

```
市场 listing（sandbox 签名面外，DK-43）
  → to_manifest（sandbox=缺省安全值）
  → 【本原语】apply_sandbox_policy（宿主策略覆盖——per-publisher 差异化）
  → load/invoke（DK-42 manifest.sandbox 覆盖 runtime 缺省）
```

## 三、待接线（排 DK-44 后，Bravo desktop 领地）

1. 安装 command 调用点：市场安装流程中 `plugin_sandbox_policy(author) → apply_sandbox_policy`
2. 策略管理 UI：settings 页 per-publisher 限额编辑（读写原语已有，纯 UI 面）

## 四、验证

- plugin_policy 3 测 + 既有回归全绿（-D warnings）
- fmt/clippy 零
- 工具教训：**root 属主文件会阻塞 cargo fmt**（Write 工具偶发 root 会话）——unlink 后 z 属主重建修复
