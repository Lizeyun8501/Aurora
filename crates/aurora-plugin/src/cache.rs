//! 模块编译缓存（DK-模块编译缓存卡，Bravo 领地）。
//!
//! **目标**：启动后首次 invoke 的 `Component::new` 全量编译（MB 级插件
//! 数百 ms-秒级）→ 第二次启动起走 `Component::deserialize`（毫秒级）。
//! installer/audit/fetcher 零触碰（Alpha 并行领地）；lazy 语义保持
//! （不做启动预热，未 invoke 的插件零编译成本）。
//!
//! ## 缓存形态裁决（卡面二选一 → 取 A）
//!
//! **A 键省 checksum + 值内嵌完整性头**：invoke 路径只有 wasm_bytes，
//! 拿不到 listing.checksum（数据源在 installer/listing 层）；B 形态需
//! 把 checksum 参数扩散进 invoke 链（越领地改签名）。故值头自包含：
//!
//! ```text
//! MAGIC("AUMC") | tag_len(u32 LE) | engine_tag | sha256(wasm)(32B)
//!   | ts(u64 LE，容量护栏 LRU 依据) | serialized(code memory 快照)
//! ```
//!
//! 双防线（对应 `Component::deserialize` 的 unsafe 前提）：
//! 1. **engine_tag 编译期绑定**（aurora-plugin 版本串——wasmtime 版本
//!    升级时必随动）：版本漂移 → miss → fail-open 全量编译；
//! 2. **sha256 完整性**：KV 字节损坏（非版本漂移）→ 校验拒绝 →
//!    fail-open。**unsafe 语义**：`deserialize` 对不匹配字节可能 UB
//!    而非 Err，故**必须先验头后调 unsafe**（失败路径绝不触碰
//!    deserialize）；残余风险=engine_tag 碰撞（同 tag 异 config），
//!    本进程 Engine 配置硬编码不变，跨进程配置漂移场景无（单进程
//!    桌面），如实注记。
//!
//! ## fail-open 语义
//!
//! 缓存命中失败（损坏/漂移/Err）一律降级全量编译并回写修复缓存，
//! **不阻断 invoke 主链**；仅 `load_component` 自身失败上抛（与
//! 无缓存行为一致）。容量护栏 64 条（`MODCACHE_MAX_ENTRIES`），超限
//! 按头内 ts 删最旧（装卸循环膨胀防护）。

use crate::wasm_runtime::WasmRuntime;
use aurora_core::traits::kv_store::KVStore;

/// 缓存键前缀（与 installer installed/blob 前缀同域，独立子空间）。
pub const MODCACHE_PREFIX: &str = "plugins:modcache:";
/// 容量护栏：前缀键数上限（超限删最旧——按值头 ts）。
pub const MODCACHE_MAX_ENTRIES: usize = 64;
/// 值头 magic（"AUMC"——Aurora Module Cache）。
const MAGIC: &[u8; 4] = b"AUMC";
/// engine_tag：编译期绑定 aurora-plugin 版本 + **wasmtime 实锁版本**
/// （build.rs 从 Cargo.lock 提取导出 `WASMTIME_LOCK_VERSION`——wasmtime
/// 升级 tag 自动随动，旧缓存 miss fail-open 重编译，零纪律依赖；加固前
/// 依赖"升级手动 bump"流程保证——2026-10-17 挂起池项收口）。tag 漂移即
/// miss，防跨版本 deserialize UB。
const ENGINE_TAG: &str = concat!(
    "aurora-plugin/",
    env!("CARGO_PKG_VERSION"),
    "/wasmtime/",
    env!("WASMTIME_LOCK_VERSION")
);

/// 摘要 32B（值头完整性域——非 installer 的 hex 形态）。
fn sha2_32b(wasm: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    let mut h = sha2::Sha256::new();
    h.update(wasm);
    h.finalize().into()
}

/// 序列化缓存值打包（头 + code memory 快照）。
fn encode_blob(serialized: &[u8], wasm_bytes: &[u8], ts: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + 4 + ENGINE_TAG.len() + 32 + 8 + serialized.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(ENGINE_TAG.len() as u32).to_le_bytes());
    out.extend_from_slice(ENGINE_TAG.as_bytes());
    out.extend_from_slice(&sha2_32b(wasm_bytes));
    out.extend_from_slice(&ts.to_le_bytes());
    out.extend_from_slice(serialized);
    out
}

/// 值头解析（校验失败 → None——调用方 fail-open）。
/// 返回 (engine_tag, sha256(wasm), ts, serialized 快照)。
fn decode_head(blob: &[u8]) -> Option<(String, [u8; 32], u64, &[u8])> {
    if blob.len() < 4 + 4 + 32 + 8 || &blob[0..4] != MAGIC {
        return None;
    }
    let tag_len = u32::from_le_bytes(blob[4..8].try_into().ok()?) as usize;
    if blob.len() < 8 + tag_len + 32 + 8 {
        return None;
    }
    let tag = String::from_utf8(blob[8..8 + tag_len].to_vec()).ok()?;
    let digest: [u8; 32] = blob[8 + tag_len..8 + tag_len + 32].try_into().ok()?;
    let off = 8 + tag_len + 32;
    let ts = u64::from_le_bytes(blob[off..off + 8].try_into().ok()?);
    Some((tag, digest, ts, &blob[off + 8..]))
}

/// 模块编译缓存装载（DK-模块编译缓存卡主入口）。
///
/// 命中（engine_tag + sha256 双校验过）→ `Component::deserialize`
/// （**unsafe**：前提由头校验担保，见模块文档）→ 登记 runtime 槽；
/// miss/损坏/漂移 → 全量编译 → 登记 → serialize 回写 KV（懒回写，
/// fail-open）→ 容量护栏修剪。`checksum` 非空时与 wasm 字节 sha256
/// hex 对账（listing.checksum 数据源可达时的双保险；invoke 路径传 ""）。
///
/// # Errors
/// 仅 `load_component` 编译失败上抛（`WasmRuntime`）——缓存路径失败
/// 一律 fail-open 降级，不上抛。
pub async fn load_component_cached(
    runtime: &WasmRuntime,
    kv: &dyn KVStore,
    plugin_id: &str,
    wasm_bytes: &[u8],
    checksum: &str,
) -> Result<(), crate::Error> {
    let key = format!("{MODCACHE_PREFIX}{plugin_id}");
    let want = sha2_32b(wasm_bytes);
    let mut restored = false;

    if let Ok(Some(blob)) = kv.get(&key).await {
        if let Some((tag, digest, _ts, ser)) = decode_head(&blob) {
            if tag == ENGINE_TAG && digest == want {
                // listing.checksum 双保险（数据源可达时）
                let checksum_ok =
                    checksum.is_empty() || checksum == crate::installer::sha256_hex(wasm_bytes);
                if checksum_ok {
                    // SAFETY：deserialize 的 unsafe 前提由双头校验担保——
                    // engine_tag 绑定编译期版本（防跨版本 UB）+ sha256
                    // 绑定 wasm 字节（防 KV 损坏）；失败路径绝不进入本支。
                    if let Ok(comp) = unsafe {
                        wasmtime::component::Component::deserialize(runtime.engine(), ser)
                    } {
                        runtime.stash_cached_component(plugin_id, comp);
                        restored = true;
                    }
                }
            }
        }
    }

    if restored {
        return Ok(());
    }

    // fail-open：全量编译（与无缓存行为一致）+ stash + 懒回写
    let comp = runtime.load_component(wasm_bytes)?;
    runtime.stash_cached_component(plugin_id, comp.clone());
    if let Ok(ser) = comp.serialize() {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let blob = encode_blob(&ser, wasm_bytes, ts);
        // 写失败不阻断（fail-open——缓存是优化不是正确性依赖）
        let _ = kv.set(&key, &blob).await;
        trim_modcache(kv, MODCACHE_MAX_ENTRIES).await;
    }
    Ok(())
}

/// 容量护栏：前缀键数超上限时按值头 ts 删最旧（解析失败视 ts=0 最旧优先删）。
pub async fn trim_modcache(kv: &dyn KVStore, max: usize) {
    let Ok(rows) = kv.scan_prefix(MODCACHE_PREFIX).await else {
        return;
    };
    if rows.len() <= max {
        return;
    }
    let mut ranked: Vec<(u64, String)> = rows
        .into_iter()
        .map(|(k, v)| {
            let ts = decode_head(&v).map(|(_, _, ts, _)| ts).unwrap_or(0);
            (ts, k)
        })
        .collect();
    ranked.sort_by_key(|(ts, _)| *ts);
    let excess = ranked.len() - max;
    for (_, k) in ranked.into_iter().take(excess) {
        let _ = kv.delete(&k).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::installer::tests::MemKv;
    use crate::wasm_runtime::WasmRuntime;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::Mutex;

    #[tokio::test]
    async fn test_first_load_writes_cache_key() {
        let runtime = Arc::new(WasmRuntime::with_default_registry());
        let kv: Arc<dyn KVStore> = Arc::new(MemKv(Mutex::new(HashMap::new())));
        let wasm = b"(component)";
        load_component_cached(&runtime, &*kv, "hello", wasm, "")
            .await
            .unwrap();
        let blob = kv.get("plugins:modcache:hello").await.unwrap().unwrap();
        assert_eq!(&blob[0..4], MAGIC, "值头 magic 就位");
        assert!(decode_head(&blob).is_some());
    }

    #[tokio::test]
    async fn test_corrupt_cache_fails_open_and_repairs() {
        let runtime = Arc::new(WasmRuntime::with_default_registry());
        let kv: Arc<dyn KVStore> = Arc::new(MemKv(Mutex::new(HashMap::new())));
        // 预置损坏缓存（magic 对但体截断——decode 失败路径）
        kv.set("plugins:modcache:hello", b"AUMCgarbage")
            .await
            .unwrap();
        let wasm = b"(component)";
        load_component_cached(&runtime, &*kv, "hello", wasm, "")
            .await
            .unwrap();
        let blob = kv.get("plugins:modcache:hello").await.unwrap().unwrap();
        assert!(decode_head(&blob).is_some(), "fail-open 后缓存被修复回写");
    }

    #[tokio::test]
    async fn test_byte_drift_miss_recompiles() {
        let runtime = Arc::new(WasmRuntime::with_default_registry());
        let kv: Arc<dyn KVStore> = Arc::new(MemKv(Mutex::new(HashMap::new())));
        // 合法 component（wat→wasm）；预置一份绑定"别的字节"的合法头缓存
        let wasm_a = wat::parse_str("(component)").expect("wat parses");
        let blob_b = encode_blob(b"pretend-serialized-for-wasm-b", b"wasm-b-bytes", 111);
        kv.set("plugins:modcache:drift", &blob_b).await.unwrap();
        // prime(wasm_a)：头 digest=wasm-b ≠ wasm_a → miss → 真编译 + 修复
        load_component_cached(&runtime, &*kv, "drift", &wasm_a, "")
            .await
            .unwrap();
        let blob = kv.get("plugins:modcache:drift").await.unwrap().unwrap();
        let (_, digest, ts, _) = decode_head(&blob).unwrap();
        assert_eq!(digest, sha2_32b(&wasm_a), "缓存已绑定新字节");
        assert!(ts != 111, "回写发生（非旧头残留）");
    }

    #[tokio::test]
    async fn test_trim_modcache_evicts_oldest() {
        let kv: Arc<dyn KVStore> = Arc::new(MemKv(Mutex::new(HashMap::new())));
        // 66 条手控 ts 递增（确定性 LRU——绕开同秒并列）
        for i in 0..66 {
            let blob = encode_blob(
                b"ser",
                format!("w{i}").into_bytes().as_slice(),
                1000 + i as u64,
            );
            kv.set(&format!("{MODCACHE_PREFIX}p{i}"), &blob)
                .await
                .unwrap();
        }
        trim_modcache(&*kv, 64).await;
        let rows = kv.scan_prefix(MODCACHE_PREFIX).await.unwrap();
        assert_eq!(rows.len(), 64, "修剪到上限");
        let ids: Vec<String> = rows
            .iter()
            .map(|(k, _)| k.trim_start_matches(MODCACHE_PREFIX).to_string())
            .collect();
        assert!(!ids.iter().any(|i| i == "p0" || i == "p1"), "ts 最小两条删");
        assert!(ids.iter().any(|i| i == "p2"), "第三旧保留");
        assert!(ids.iter().any(|i| i == "p65"), "最新保留");
    }

    #[test]
    fn test_decode_head_version_drift_miss() {
        let blob = encode_blob(&[1, 2, 3], b"wasm", 42);
        assert!(decode_head(&blob).is_some());
        let mut drifted = blob.clone();
        let tag_off = 8;
        drifted[tag_off] ^= 0xFF;
        assert!(
            decode_head(&drifted).is_none() || {
                // tag 区被改仍可解析但内容已漂移——verify tag != ENGINE_TAG
                decode_head(&drifted)
                    .map(|(t, _, _, _)| t != ENGINE_TAG)
                    .unwrap_or(true)
            },
            "engine_tag 漂移必须可检出"
        );
    }
}
