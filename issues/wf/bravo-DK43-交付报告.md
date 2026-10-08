# DK-43 交付报告：市场 listing→manifest 的 sandbox 传播（DK-42 启用面闭环）

> 执行：Bravo 2026-10-08 · 依据：alpha-DK42-验收复核"插件市场迭代（DK-14 面启用
> manifest 级限额）"候选 + 窗口期授权（TA「继续推进」，无在途卡）
> Commit：见 git log（feat(DK-43)）

## 落地面

### 1. PluginListing.sandbox（市场差异化限额的数据载体）
- `#[serde(skip, default = "default_sandbox")]`——**签名面外**设计：
  `canonical_bytes()` 不含 sandbox（bincode skip）——**签名认证代码来源，
  限额是宿主策略**，已签名旧 listing 的签名验证不受新增字段影响
- serde default = 缺省安全值（2s/64MiB），旧市场清单 JSON 零迁移

### 2. listing→manifest 传播（安装路径接线）
- `PluginListing::to_manifest(entry, permissions, hooks, block_types)`——
  市场安装时构造 PluginManifest，`sandbox` 随 listing 传播
- 宿主加载市场插件即获得 per-plugin 限额（DK-42 的 invoke 覆盖链路自动生效）

## DoD 验证

| 测试 | 断言 | 结果 |
|---|---|---|
| dk43_listing_to_manifest_sandbox_propagation | listing.sandbox → manifest.sandbox 传播 + 权限/钩子面正确 | ✅ |
| dk43_sandbox_outside_signature_surface | 仅 sandbox 不同的两个 listing canonical_bytes 完全一致（签名稳定） | ✅ |
| dk43_listing_sandbox_serde_default | 旧市场清单 JSON（无 sandbox）反序列化 = 缺省安全值 | ✅ |

## 验证矩阵

| 门 | 结果 |
|---|---|
| TEST（全量 workspace --exclude desktop） | 0 ✅ |
| CLIPPY | 0 ✅（workspace 全量 + plugin 单独终验，-D warnings） |
| FMT | 0 ✅ |
| DESK | CI main 面为准 |

## 说明
- 规模 S：listing 字段+to_manifest ~35 行、测试 ~55 行
- 语义注记：签名面外设计使"市场运营调整限额策略"无需重新签名代码包
  （宿主策略与代码完整性正交）——插件市场迭代的限额运营面就绪
