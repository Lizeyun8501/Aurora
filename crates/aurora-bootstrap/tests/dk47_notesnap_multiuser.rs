//! DK-47: notesnap 多用户隔离（末案）——per-user KV key 行为级测试。
//!
//! 四裁决落地验证：隔离不立账号（裁决 1）/ KVStore trait 不动（裁决 2）/
//! key 统一带段 default 含段（裁决 3）/ 惰性迁移 get 单点（裁决 4）。
//! DoD1 双用户隔离 e2e / DoD2 旧数据零丢失（惰性迁移）/ DoD4 迁移幂等。
//! DoD3 default 零回归由 isomorphic_write_path 对拍（key 形态断言升级）覆盖。

use aurora_core::app_core::{
    get_notesnap_with_migration, notesnap_key, DEFAULT_USER_ID, LEGACY_NOTESNAP_PREFIX,
};
use aurora_core::l1_infrastructure::storage_engine::MemoryKVStore;
use aurora_core::traits::kv_store::KVStore;

/// DoD1: 双用户隔离——同 note_id 不同 user_id 快照互不干扰。
/// user A 写 → user B 读不得见（不立账号，仅 KV key 段隔离）。
#[tokio::test]
async fn dk47_dual_user_isolation() {
    let kv = MemoryKVStore::default();
    let snap_a = b"user-a-snapshot-bytes".to_vec();
    let note_id = "note-shared-id";

    // user A 写入快照（写路径落 per-user 新 key）
    kv.set(&notesnap_key("user-a", note_id), &snap_a)
        .await
        .expect("kv set user-a");

    // user B 读同 note_id：不得见 A 的快照（隔离）
    let seen_by_b = get_notesnap_with_migration(&kv, "user-b", note_id)
        .await
        .expect("migration read user-b");
    assert!(seen_by_b.is_none(), "user B 不得读见 user A 的快照");

    // user A 仍可读回（内容一致）
    let seen_by_a = get_notesnap_with_migration(&kv, "user-a", note_id)
        .await
        .expect("migration read user-a");
    assert_eq!(
        seen_by_a.expect("user A 快照须在"),
        snap_a,
        "user A 读回内容须一致"
    );

    // default 段与 user-a 段互不干扰
    let seen_by_default = get_notesnap_with_migration(&kv, DEFAULT_USER_ID, note_id)
        .await
        .expect("migration read default");
    assert!(seen_by_default.is_none(), "default 段不得读见 user-a 快照");
}

/// DoD2: 旧数据零丢失——legacy 单用户 key（`notesnap:{note_id}`）惰性
/// 迁移到 default 段：新路径读 → 新 key 命中 + 旧 key 已删 + 内容一致。
#[tokio::test]
async fn dk47_legacy_lazy_migration() {
    let kv = MemoryKVStore::default();
    let legacy_bytes = b"legacy-single-user-snapshot".to_vec();
    let note_id = "note-legacy-owner";

    // 构造历史数据（单用户时代落盘形态——无 user 段）
    let legacy_key = format!("{LEGACY_NOTESNAP_PREFIX}{note_id}");
    kv.set(&legacy_key, &legacy_bytes)
        .await
        .expect("kv set legacy");

    // 新路径读（default 用户）→ 触发惰性迁移并返回内容
    let got = get_notesnap_with_migration(&kv, DEFAULT_USER_ID, note_id)
        .await
        .expect("migration read");
    assert_eq!(
        got.expect("legacy 快照须被迁移读出"),
        legacy_bytes,
        "迁移后内容须一致（零丢失）"
    );

    // 新 key 命中（迁移目标落位 default 段）
    let new_key = notesnap_key(DEFAULT_USER_ID, note_id);
    let migrated = kv
        .get(&new_key)
        .await
        .expect("kv get new key")
        .expect("新 key 须存在（迁移落位）");
    assert_eq!(migrated, legacy_bytes, "新 key 内容须与 legacy 一致");

    // 旧 key 已删（搬迁语义——非复制）
    let legacy_left = kv
        .get(&legacy_key)
        .await
        .expect("kv get legacy after migration");
    assert!(legacy_left.is_none(), "旧 key 须已删除（搬迁非复制）");
}

/// DoD4: 迁移幂等——同一旧数据迁移后再次读不重复搬移
/// （旧 key 已删天然幂等：二次读直接命中新 key，无第二次 set/delete）。
#[tokio::test]
async fn dk47_migration_idempotent() {
    let kv = MemoryKVStore::default();
    let legacy_bytes = b"idempotent-snapshot".to_vec();
    let note_id = "note-idem";
    let legacy_key = format!("{LEGACY_NOTESNAP_PREFIX}{note_id}");
    let new_key = notesnap_key(DEFAULT_USER_ID, note_id);

    kv.set(&legacy_key, &legacy_bytes)
        .await
        .expect("kv set legacy");

    // 第一次读 → 迁移发生
    let first = get_notesnap_with_migration(&kv, DEFAULT_USER_ID, note_id)
        .await
        .expect("first read");
    assert_eq!(first.expect("first"), legacy_bytes);

    // 迁移后状态：legacy 已删、新 key 在位
    assert!(
        kv.get(&legacy_key).await.expect("get legacy").is_none(),
        "迁移后 legacy 须已删"
    );

    // 第二次读（幂等路径）——内容一致、legacy 不复活
    let second = get_notesnap_with_migration(&kv, DEFAULT_USER_ID, note_id)
        .await
        .expect("second read");
    assert_eq!(second.expect("second"), legacy_bytes, "二次读内容须一致");
    assert!(
        kv.get(&legacy_key).await.expect("get legacy 2").is_none(),
        "二次读后 legacy 仍不得复活（无重复搬移副作用）"
    );
    assert_eq!(
        kv.get(&new_key).await.expect("get new").expect("new key"),
        legacy_bytes,
        "新 key 内容稳定"
    );
}

/// key 形态口径（裁决 3）：统一带段、default 含段、legacy 前缀无段。
#[test]
fn dk47_key_shape_contract() {
    assert_eq!(
        notesnap_key("default", "n1"),
        "notesnap:default:n1",
        "default 用户 key 须含 default 段"
    );
    assert_eq!(
        notesnap_key("ua", "n1"),
        "notesnap:ua:n1",
        "per-user key 须带 user 段"
    );
    assert_eq!(LEGACY_NOTESNAP_PREFIX, "notesnap:");
}
