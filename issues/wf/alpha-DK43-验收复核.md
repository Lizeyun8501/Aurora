# Alpha · DK-43 验收复核 — PASS（插件市场启用面闭环）

**复核对象**: 5b0efb4（Bravo 12:42 直推 main）｜**复核**: Alpha 2026-10-08 17:25
**链路**: alpha-V1阶段收口报告.md 方向一（插件市场优先）→ Bravo 自主推进 → 本验收

---

## 一、结论：**PASS——DK-42 能力的市场消费面闭环**

| 门 | 结果 | 说明 |
|---|---|---|
| diff 审 | ✓ | marketplace.rs +86 纯增量：PluginListing.sandbox + to_manifest 安装接线；**零越界** |
| DoD 复跑 | ✓ **本地三测+marketplace 83 测全绿** | 见下 |
| 四门 | ✓ | workspace check Finished + clippy 零 + **CI badge passing**（5b0efb4）|

## 二、三 DoD 确认

1. **传播**：`listing.sandbox → manifest.sandbox`（安装路径 to_manifest 接线）——DK-42 invoke 覆盖链路自动生效 ✓
2. **签名面外**（本卡设计亮点）：`#[serde(skip, default)]`——canonical_bytes 不含 sandbox；DoD2 断言「仅 sandbox 差异的两 listing canonical 字节一致」——**签名稳定性硬锚定**。架构判断正确：签名认证代码来源，限额是宿主策略，两者正交
3. **serde 缺省**：旧市场清单 JSON（无 sandbox 字段）反序列化=缺省安全值（2s/64MiB），零迁移 ✓

## 三、注记（非阻断）

**宿主策略设置入口未落地**：`#[serde(skip)]` 意味着 listing 从市场 JSON 拉取后 sandbox 恒为 default——「市场差异化限额」的完整闭环缺最后一环：**宿主侧策略配置点**（如 settings 里 per-publisher/per-marketplace 限额映射，安装时覆盖 listing.sandbox）。当前 default 安全值保底正确，策略入口属产品迭代面（UI 设置页或市场审核后台）——建议下一卡候选或并入插件市场正式迭代。

## 四、流水线状态

- DK-40a→43 全链闭环（同步主线+插件市场启用面）
- Bravo 连续三卡直推 main（DK-42/43 + 开工令响应），零分叉零打回
- **裁决清单全关账**：5 项全部闭环（38 缓派/40b 收口/SandboxLimits/分支清理/工作流修正实证）
- 下一迭代方向：收口报告四案待排序（插件市场深化/UI 同步体验/relay/notesnap）
