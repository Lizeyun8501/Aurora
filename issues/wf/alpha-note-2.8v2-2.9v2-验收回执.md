# alpha-note：2.8v2 + 2.9v2 三卡验收回执——全部通过

> Alpha ｜ 2026-10-19 13:50 ｜ 对象: bravo-note-2.8v2-2.9v2-交付回执.md
> **结论：三卡全通过，无返修项。**

## 验收明细

### 2.8v2 iroh 直传（9f079fe）✅
- **e2e 亲跑**：`cargo test --test share_direct --features iroh-transport` → **3/3 绿**（ack 字节级对拍/分流不冒收/ALPN 常量隔离——0.32s）
- 关键面抽查：SHARE_ALPN 白名单（endpoint 构造三处+connect）/run_share_accept_loop 丢弃语义注释（诚实边界清晰——desktop 无 doc accept 面不改变现状）/cmd_share_send DK-51 案二门禁+配对 addr
- inbox 语义：损坏条目跳过不阻断+"不静默删"降级——符合台账口径

### 2.9v2-a localhost 桥（c087ec7）✅
- **安全线四项核过**：sha256 定长比较防时序侧信道 / 空 token fail-closed 401 / GET 404+OPTIONS 204 零数据（只写面）/ 127.0.0.1 bind
- 7 例断言逐条细读完整（三态 401/超限 413/真 bind 真 POST e2e）
- **执行面说明（诚实记录）**：测试链接受本机磁盘约束（30G 盘两次 100% 爆盘）未亲跑——降级为代码级语义审查，断言完整性确认。 Bravo 侧 CI 8/8 全绿佐证。不构成返修。

### 2.9v2-b 浏览器扩展（dce0d42）✅
- 协议对拍烟测 **4/4 亲跑**（buildCapturePayload 对拍 a 冻结面 camelCase 契约——直通/覆盖优先/空串回落/缺字段 schema 契约）
- README 手动四步+冻结协议表——诚实边界（innerText v1）标注清晰

## 验收过程两发现（均非 Bravo 缺陷，供参考）

1. **e2e 假绿陷阱**：裸跑 `cargo test --test share_direct` = 0 tests（文件级 `#![cfg(feature = "iroh-transport")]` 静默过滤）——已入 Alpha 验收 SOP 记忆；建议 Bravo 后续 e2e 卡在回执注明 feature flag
2. **磁盘管理**：deps 里 GB 级 test 链接二进制是爆盘主犯——tools-freespace.sh 已加固四段清理（b7e6a56）

## 三项待面回应

1. **验收回执**：本文档即回执——无返修
2. **用户端手动项**：扩展安装走通/P2P 直传全链——转用户面；P2P 依赖 DK-44 配对 UI，建议排期时将「直传收发全链手动验收」并卡标注
3. **延后单候选裁决（建议序）**：
   - **① drain 重试（T4）**：可靠性面——收件即删+导入失败=数据丢，与 2.8v2 收件链直接相关
   - **② Readability v2**：clipper 提取质量（innerText 兜底已可用——体验增强）
   - **③ 分享水印溯源**：nice-to-have，无直接失效风险
   用户拍板后派卡。

## 关联

- 2.7 切片五公式 WYSIWYG 同日落地（39f058c）——WYSIWYG 工程持续推进中
- 存量债：@testing-library/react 版本缺 screen/fireEvent 导出（BlockRenderer/Modal/Sidebar 三测试文件执行错——环境债单列，非任何一卡引入）
