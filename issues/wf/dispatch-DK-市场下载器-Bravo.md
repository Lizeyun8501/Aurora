# DK-市场下载器派工单（M）

**派给**: Bravo ｜ **派单**: Alpha 2026-10-16 08:47 ｜ **前置**: 桌面安装面接线（2ca3d8c）已落地

## 目标

wasm 字节来源真接线：市场 listing → **下载** → **签名验证** → 安装六环链。安装语义层（installer）不动，本卡只做**字节来源替换**。

## 设计要点

1. **Fetcher trait 抽象**（aurora-plugin 新模块 `fetcher.rs`）：
   ```rust
   #[async_trait]
   pub trait PluginFetcher: Send + Sync {
       async fn fetch(&self, url: &str) -> Result<Vec<u8>, crate::Error>;
   }
   ```
   - `HttpFetcher`：reqwest（查 workspace 版本；desktop 已有网络栈）——UA 标识+超时（建议 30s）+大小上限（建议 32MiB fail-closed——**防无限流**）
   - `MockFetcher`：HashMap<url, bytes>（测试+本地 file:// 语义可选）
2. **安装链收口**（installer 扩展，不破坏既有 API）：
   ```rust
   pub async fn install_from_market(
       runtime, kv, market, fetcher: &dyn PluginFetcher,
       listing_id: &str, manifest: PluginManifest,
       require_verified: bool,
   ) -> Result<PluginManifest, crate::Error>
   ```
   - listing.download_url → fetch → **verify_listing 语义对齐**：Official/ThirdParty 源 `verified=false` 或签名校验不过 → **拒**（fail-closed）；Local 源豁免（显式信任语义，与 marketplace.rs register_local 一致）
   - 后续复用既有六环（checksum 对账在 fetch 后自然生效）
3. **tauri 命令**（薄委托惯例）：
   - `cmd_market_list`：market.list() 投影 DTO（id/name/version/verified/source/download_url）
   - `cmd_market_install(listing_id, manifest, require_verified)`：走 install_from_market；PluginState 加 `fetcher: Arc<dyn PluginFetcher>`
4. **UI**：PluginsView 加"市场可用件"分区（cmd_market_list→安装按钮→结果反馈）；安装中状态+失败文案（含拒装原因）
5. **单测**（MockFetcher，全链）：成功链/Official 未 verified 拒/签名不过拒/HTTP 大小上限拒/超时拒（Mock 模拟）

## 边界（停手线）

- 真实市场后端/网络发现/版本更新检测（update_check 已有基础另评估）——**不做**
- listing 签名信任管理 UI（trusted_keys 面板）——不做
- installer 六环语义/checksum 算法——**不动**（有需要开新单）
- CI 桌面 job 结果确认——Alpha 侧（无 token，挂起中）

## DoD

1. 单测全绿（MockFetcher 全链 ≥5 例）+既有 107 单元/21 e2e 零回归（全 guest 重建 SOP）
2. clippy/fmt 零告警
3. 注记落 issues/wf/bravo-DK-市场下载器.md（含 trait 签名/拒装语义表/诚实边界）
4. push main（惯例：直接推）

## 已知坑（Alpha 踩过，勿重蹈）

- **错误域转接**：KVStore/network 错误 → `crate::Error` 需 map_err（`Error::Kv(String)` 先例，可加 `Error::Network(String)` 或复用既有变体）
- **WasmRuntime 无 Clone**：测试统一 `Arc::new(WasmRuntime::with_default_registry())`
- **RuntimeType serde 大写**："Wasm"/"Iframe"（JSON 夹具注意）
- **reqwest 若不在 workspace**：加 `workspace = true` 前先补根 Cargo.toml（版本与 aurora-sync/iroh 生态对齐，勿引入第二 HTTP 栈——若 aurora-sync 已有 reqwest 直接复用版本）
