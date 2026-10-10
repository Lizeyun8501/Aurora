# DK-46 Alpha 验收注记（关账）

**验收**: Alpha 2026-10-10 11:50 ｜**结论**: ✅ 通过关账 ｜**commits**: 2d3d43b + 3 发 CI 修复（3ca5deb/2245dd0/0b6368e）→ 终版 0b6368e

## 终证渠道

- **CI 五检查全绿**（0b6368e）：Test / MSRV / desktop-check / Clippy / Rustfmt
- Bravo 自修 3 发 CI 红：dead_code/unused_mut（-D warnings）、测试挂死护栏（wait_online 30s 封顶+150s 整体）、自签 TLS 信任根（UnknownCA→CaTlsConfig::custom_roots）——修复均有注记，属环境适配非设计返工

## DoD 核对（代码级精读）

| DoD | 结论 | 依据 |
|---|---|---|
| 1. relay_mode 注入生效+Disabled 零回归 | ✅ | RelayModeConfig 四档枚举（不泄漏 iroh 类型）+ injection_semantics 测试 matches! 断言；Disabled 与 new 等价 |
| 2. 本地会合 e2e（零外部依赖） | ✅ | iroh_relay::server::Server::spawn 进程内 relay；**clear_ip_transports 禁直连**（官方 patchbay 同款——会合路径可证不被直连捷径污染）；message_arrival 测 success+received_bytes>0 |
| 3. 不可达显式错误 | ✅ | unreachable 测试直测 "not connected within"；Custom 坏 URL 解析显式 Err；wait_online 30s 封顶防死等 |
| 4. 回归+四门 | ✅ | CI 全绿 |

## 设计亮点

1. **custom_roots 无 feature 门控**：本地自签 relay 证书显式信任（CI 裸构建生效）——比旧 insecure_skip_verify（test-utils 门控）干净的生产路径
2. **完成信号超时降级裁决**（详实注记）：强 relay 路径下 noq bi 双流交织的完成信号可延迟（iroh 1.0.3 官方 e2e 仅覆盖 uni 单流）——降级 warn 不阻断同步结果，**数据完整性由 CRDT 收敛保证，不可达路径仍显式 Err**——边界清楚，底线守住
3. **wait_home_relay_url vs wait_online 分层**：前者证「连上 relay」，后者证「relay 上已注册可达」——会合 connect 前置条件明确
4. 领地干净✓：iroh_transport.rs 主战场+新 e2e 文件+ci.yml 护栏，禁触面零触碰

## 流水线推进

DK-46 关账 → 四案排序 2/1/3 全收官 → **末案 notesnap 多用户（需 token）**待用户补给后派发。
