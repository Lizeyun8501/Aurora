# bravo-DK-市场下载器 注记（Bravo 2026-10-16）

**commit**: 见 git log（feat(DK-市场下载器)）｜ **前置**: 2ca3d8c 桌面安装面接线

## trait 签名

```rust
// crates/aurora-plugin/src/fetcher.rs
#[async_trait]
pub trait PluginFetcher: Send + Sync {
    async fn fetch(&self, url: &str) -> Result<Vec<u8>, crate::Error>;
}
pub const MAX_PLUGIN_BYTES: usize = 32 * 1024 * 1024;  // 32MiB fail-closed
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(30);
pub const USER_AGENT: &str = "Aurora-PluginFetcher/<pkg-version>";
```

- `HttpFetcher::default()`：reqwest 0.12（与 aurora-sync 同款 default-features=false + rustls-tls——不引第二 HTTP 栈；显式版本未走 workspace.dependencies，**不动 aurora-sync 领地**）
- `HttpFetcher::with_limits(max_bytes, timeout)`：测试可调小（本机无外网依赖，用不可路由地址验证错误域转接）
- `MockFetcher`：HashMap<url, bytes>，miss → `Error::Network`；超限 → `Error::InvalidInput`

## 拒装语义表（install_from_market verified 环）

| source | listing.verified | require_verified | 结果 |
|---|---|---|---|
| Local | 任意 | 任意 | 放行（本地豁免） |
| Official/ThirdParty | true | 任意 | 放行 |
| Official/ThirdParty | false | true | **拒**（SignatureInvalid"not verified"——拒装不花网络预算，verified 环先于下载） |
| Official/ThirdParty | false | false | 放行（显式降权通道，调用方自担——UI 默认 true 不暴露） |

require_verified=false 是**显式降权**口（测试/开发用），非 verified=false 的等价放行——两者组合语义见上表。

## 六环复用与字节来源

- `install_from_market` = verified 环（新增）+ fetcher 下载 + `install_plugin` 六环原样复用（installer 语义**零改动**）
- 下载失败（Network/超限）传播上抛，六环不触达、KV 无残留（单测断言）
- `Error::Network(String)` 新变体（fetcher 传输层失败转接；超限走 InvalidInput 与既有 checksum 语义一致）

## 桌面接线

- `PluginState.fetcher: Arc<dyn PluginFetcher>`（默认 HttpFetcher；测试/离线可注入 Mock）
- `cmd_market_list`（sync，listing 全量）+ `cmd_market_install`（require_verified 缺省 true）
- UI PluginsView 市场分区：listing 行（source/verified 徽标）+ 安装按钮（安装中禁用态）+ 失败文案透传拒装原因
- `MarketListing` manifest 构造走 UI 侧同名字段投影（permissions 空集——权限授予链另卡）

## 单测（lib 116 = 107 + 9 增量）

- fetcher 3：hit/miss/超限 fail-closed + Http 错误域转接（不可路由地址）
- installer 6：Local 豁免成功链 / Official 未 verified 拒 / require_verified=false 降权放行 / Official verified=true 过 / 下载 miss 传播+KV 无残留 / 超限拒于 checksum 前

## 诚实边界

- 真实市场后端/网络发现/更新检测——未做（停手线）
- trusted_keys UI——未做（verify_listing 信任链已有 API，UI 另卡）
- HttpFetcher 未做真实 HTTP 全链单测（本机无外网依赖原则；错误域转接用不可路由地址覆盖）——CI 环境亦无外网，真实网络链路待市场后端卡定 Host 后补
- tauri 面本地无 GTK 不可 check（gdk-sys build 噪声）——CI desktop-check 兜底
- 本卡工作区曾现 root 属主文件（fetcher.rs 初建即 root）——rm 重建夺回（既有惯例）

## 验证矩阵

| 门 | 结果 |
|---|---|
| lib 单元 | 116 ok（107+9） |
| e2e | 21 ok（全 guest 重建 SOP 后） |
| clippy -p aurora-plugin | 0 |
| clippy workspace(excl desktop) | 0 |
| fmt --check | 0 |
| tsc apps/desktop | 0 |

**坑与 SOP**：0.3.0 合约 bump 后 guest wasm 必须全量重建（16 例 e2e 假挂=`0.2.0 instance not found`——Alpha 注记 SOP 实证复现）；DesktopShell invoke 为 `InvokeFn | null`，新接 View props 必须对齐 nullable+内部判空守卫（TrashView 惯例）。
