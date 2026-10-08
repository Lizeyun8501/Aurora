# Alpha · relay 会合预探 — 第 3 案启动材料（API 可行性确认）

**撰写**: Alpha 2026-10-08 21:25｜**性质**: 预探文档（API 摸底+DoD 建议），不自行开工
**依据**: iroh-relay 1.0.3 源码（本地 registry）+ iroh 1.0.3 Endpoint 面——V1 剩余唯一技术缺口

---

## 一、API 可行性：**成立——本地 relay 会合测试基建可零外部依赖搭建**

| API | 来源 | 用途 |
|---|---|---|
| `iroh_relay::server::Server::spawn(config: ServerConfig)` | server.rs L691 | **本地 relay server 进程内 spawn**——测试无需真实部署 |
| `Endpoint::builder().relay_mode(RelayMode)` | endpoint.rs L153+ | `Disabled/Default/Staging/Custom(TransportConfig::Relay)`——节点指定会合 relay |
| `RelayUrl`（iroh_base） | client.rs L13 | relay 地址面 |
| client.rs 连接面 | RelayUrl+SecretKey | relay 客户端注册/发现 |

**关键结论**：`spawn 本地 relay → 双节点 relay_mode(Custom(relay_url)) → 会合同步收敛`——全内存仿真可测，与 dk40c 手法同构，**DoD 可本地化**（无需物理 NAT 环境；真实 NAT 穿透验证仍留产品化阶段）。

## 二、建议拆卡（DK-45 relay 会合，规模 S-M）

**范围**：
1. aurora-sync relay 集成面：`spawn_local_relay()` 测试基建 + Endpoint relay_mode 接线（IrohTransport 构造参数扩展）
2. e2e：双节点**无法直连仿真**（可用 iroh 的 relay-only 模式强约束——RelayMode 配置禁直连路径）→ 经 relay 会合 → 同步收敛（dk40c 断言复用）
3. relay 故障面：relay 掉线后直连/hole-punch 回退（iroh 内建行为验证——文档级确认+冒烟测试）

**DoD 建议**：
1. 双节点经本地 relay 会合同步收敛（强 relay 模式）
2. relay 可用性降级不阻断（iroh 自动切换语义冒烟）
3. 回归：dk40c/dk41/44 全绿（relay 面零影响既有直连路径）

**领地**：aurora-sync iroh_transport 构造面+测试；Cargo.toml dev-dep 加 iroh-relay（server spawn 需要——确认 feature 门）

## 三、生产部署注记（远期/产品化）

- 生产 relay = 独立服务器运维项（iroh-relay server 二进制+域名+TLS 证书）——**代码面之外的部署清单**，建议 relay 卡文档附带部署手册骨架
- 官方 default relay（euw1.use1.ap1.iroh.network）可用于开发期，生产不建议依赖第三方

## 四、流水线状态

- DK-44 正式关账（CI 终证 success）
- 队列现状：**第 2 案接线（插件市场深化）已解锁**（DK-45 前置原语 a9b039f 就绪）vs **relay 预探（本文档）**——两者无依赖，TA 可并行派或按序
- relay 派发建议：**排插件市场接线之后**（TA 排序 1>2，接线卡先行；relay 材料已备随时可开工）
