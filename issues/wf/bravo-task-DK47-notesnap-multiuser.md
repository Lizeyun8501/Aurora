# Bravo Task：DK-47 notesnap 多用户隔离（末案）

> 发起：Alpha（TA 指令：notesnap 末案启动）· 2026-10-10
> 接收：Bravo
> 依据：V1 收口报告挂账卡「notesnap 多用户」——token 补给到位（TA 2026-10-10 确认），评估结论与范围裁决如下。
> 状态：**开工令生效**。直推 main，CI 四门全绿后 Alpha 验收。

## 0. 现状勘察（Alpha 已完成，Bravo 可直接引用）

- 产品栈**零用户概念**：`user_id|account_id|current_user` 在 mobile-ffi/app_core/sync_commands 三处零匹配。
- notesnap 读写两处：
  - 移动端 `mobile-ffi/src/lib.rs`：`UniffiAppCore.docs: HashMap<note_id, NoteDoc>` 单表缓存；快照 key `notesnap:{note_id}`（get_or_build 恢复 / persist_doc 持久化，约 L259/L287）。
  - 桌面端 `apps/desktop/src-tauri/src/sync_commands.rs`：同款裸 key `notesnap:{doc_id}` 直读。
- KVStore 是 trait 注入（`Arc<dyn kv_store::KVStore>`），纯 key-value，无用户维度。

## 1. 范围裁决（四条，均已定，不必再议）

1. **立隔离能力，不立账号系统**——不引入登录/账号切换 UI。user_id 作为显式注入参数，`Option<String>`，`None`/缺省 = `"default"`。
2. **KVStore trait 不改**——多用户在调用方（key 构造层）解决。trait 加 user 参数是全局地震，禁止。
3. **key 统一带段**：`notesnap:{user_id}:{note_id}`（user_id = default 时同样带段，即 `notesnap:default:{note_id}`）。
4. **旧数据惰性迁移**：读新 key miss → 读旧 key（`notesnap:{note_id}`）→ 命中则「写新 + 删旧」一次完成搬移。理由：notesnap 全部是按 note_id 精确读写，**无列表/扫描操作**，惰性迁移面收敛在 get 路径一处；无需启动批迁移。

## 2. 变更面

| 位置 | 变更 |
|---|---|
| `aurora-core/src/app_core.rs` | 最小注入面：user 上下文字段 + builder 方法（默认 "default"）。**不动其他 AppCore 逻辑** |
| `mobile-ffi/src/lib.rs` | ① docs 缓存改复合键 `(String, String)` → `(user_id, note_id)`；② get_or_build / persist_doc 的 key 构造走统一 helper；③ 惰性迁移逻辑（仅 get 路径一处） |
| `apps/desktop/src-tauri/src/sync_commands.rs` | notesnap 读写同构（key 带段 + 惰性迁移同款 helper——**helper 放 aurora-core 或 bootstrap 共享层，双端不许各写一份**） |
| `aurora-bootstrap/tests/isomorphic_write_path.rs` | 对拍断言更新：key 形态 `notesnap:default:{id}`；**双端口径一致断言保持** |

key 构造 helper 建议形态（放共享层）：

```rust
pub fn notesnap_key(user_id: &str, note_id: &str) -> String {
    format!("notesnap:{user_id}:{note_id}")
}
pub const LEGACY_NOTESNAP_PREFIX: &str = "notesnap:";
// 惰性迁移：get(new_key) miss → get(legacy_key) → 命中则 set(new)+del(legacy)
```

## 3. DoD（验收即按此核对）

1. **双用户隔离**：同 note_id 不同 user_id 快照互不干扰（e2e 直测：user A 写 → user B 读不得见）。
2. **旧数据零丢失**：default 用户旧 key 惰性迁移单测（构造旧 key 数据 → 新路径读 → 断言新 key 命中 + 旧 key 已删 + 内容一致）。
3. **default 路径零回归**：不传 user_id 全链行为等价；bootstrap 对拍测试双端 key 形态一致保持绿。
4. **迁移幂等**：同一旧数据迁移后再次读不重复搬移（旧 key 已删天然幂等，测试断言一次）。
5. CI 四门全绿（Test/MSRV/Clippy/Rustfmt + desktop-check）。

## 4. 解禁 / 禁触

**解禁**：上表四处 + 各自测试文件。
**禁触**：
- `plugin_commands.rs` / `wasm.rs` / `marketplace.rs` / `write_path` 主链（长期）
- `iroh_transport.rs`（DK-46 刚关账留观；多用户与 P2P 交叉面本卡不碰，留后续卡）
- `KVStore` trait 及 storage.rs / storage_engine.rs 实现（裁决 2）

## 5. 边界提示

- bench 落表（原挂账同名条目）**不在本卡**，另行待 token 排期。
- 若实现中发现 user_id 需要穿透到 note:{id} 元数据层——**停手回报**，那是范围升级，不开工。
