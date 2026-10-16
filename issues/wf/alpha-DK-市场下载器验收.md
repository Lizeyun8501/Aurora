# Bravo 交付验收：DK-市场下载器（9af7af9）

**验收人**: Alpha 2026-10-16 09:38 ｜ **结论**: **有保留通过**——整改项 R1 现场完成（b7221cd 后追加），整改后 119 单元+22 e2e 全绿 clippy 干净

## 逐项核查（派单 DoD 对照）

| 项 | 判定 | 证据 |
|---|---|---|
| Fetcher trait 抽象 | ✅ | `PluginFetcher` async trait；Http/Mock 双实现 |
| HttpFetcher 32MiB fail-closed | ✅ | 双防线：Content-Length 预检+实读后校验（chunked 兜底）；30s 超时；UA 标识；`with_limits` 测试可调 |
| MockFetcher 行为对齐 | ✅ | 超限同拒（测试可注入超限载荷验证拒装路径） |
| verified 环 fail-closed | ✅ | **先于下载**（不花网络预算）；Official/ThirdParty 未 verified→SignatureInvalid；Local 豁免；`require_verified=false` 显式降权调用方自担 |
| 六环零改动复用 | ✅ | install_from_market→install_plugin 尾环透传 |
| Error::Network 变体 | ✅ | 传输层失败转接 |
| 命令层接线真实性 | ✅ | PluginState.fetcher 注入（默认 Http）；cmd_market_install 真调 install_from_market（非空壳）；get_booted 惯例；require_verified 缺省 true |
| UI 市场分区 | ✅ | source/verified 徽标+安装中禁用态+拒装原因透传+nullable invoke 守卫 |
| 测试证据 | ✅ | 独立复跑 fetcher 3/3+installer 13/13（6 增量）；合并态 119+22 |
| 注记诚实度 | ✅ | 诚实边界三条如实（真实后端/trusted_keys UI/真实 HTTP 链路待后端卡）；**manifest UI 侧构造已主动披露** |
| 停手线遵守 | ✅ | 真实市场后端/更新检测/trusted_keys UI 未越界 |

## 整改项（Alpha 现场完成，不退回）

**R1（S）manifest 派生分层错位**：UI 前端硬拼 `permissions: []` manifest——非安全洞（零权限=最小暴露面+fail-closed 全链仍生效），但偏离 DK-43 `to_manifest` 传播语义（且 manifest 构造不该在 UI 层）。
**整改**：`install_from_market` 签名 `manifest: PluginManifest` → `manifest_override: Option<PluginManifest>`——None 时语义层内部 `listing.to_manifest(entry="", permissions=[], hooks=[], block_types=[])` 派生（sandbox 差异化随 listing 传播）；cmd_market_install 参数改 Option 透传；UI 退回纯 `listingId+manifest:null+requireVerified`；死类型 MarketInstallPayload 清除；测试调用点 6 处对齐。
**效果**：市场安装 manifest 派生单点收口在语义层——未来随包 manifest 卡只需填 override，UI 零改动。

## nit（不阻塞，留顺手清）

fetcher.rs `bytes.truncate(bytes.len())` 为 no-op 死代码（超限已提前 Err，此行无效果）。

## 遗留边界（转后续卡）

1. **真实市场后端卡**：listing 分发+Host 定型→HttpFetcher 真实网络链路单测（本机/CI 无外网原则下待定）
2. **trusted_keys UI 卡**：verify_listing 信任链 API 已有，管理面另做
3. **权限授予链卡**：to_manifest 的 permissions 参数目前零集——真实权限声明（随包 manifest/用户批准面）挂此卡
4. **verified 标记产生面**：publish→verify_listing→verified=true 的链路可信性归市场后端卡保证（当前防线的强度上界）
