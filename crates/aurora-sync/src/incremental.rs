//! 增量同步 (Incremental Sync)
//!
//! # 两层增量
//! 1. CRDT ops 增量：通过版本向量差异计算需传输的 op 范围 (见 [`crate::p2p::VersionVector`])。
//! 2. rsync-like 块级增量：针对大媒体文件，使用滚动哈希 ([`RollingHash`]) + 强哈希
//!    定位变化的块，仅传输差异块 ([`BlockDelta`])。
//!
//! # 压缩
//! 增量数据使用 zstd 压缩。本模块提供 mock 压缩接口 ([`IncrementalSync::compress`]
//! 为 identity 实现)，真实实现可对接 `zstd` crate，公开签名不变。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tracing::debug;

/// 默认块大小 (4 KiB)。
pub const DEFAULT_BLOCK_SIZE: usize = 4096;

/// 滚动哈希 (Adler-32 风格)。
///
/// 用于 rsync 块匹配的弱哈希：计算快但可能存在碰撞，
/// 命中后再用 [`strong_hash`] 二次确认。
#[derive(Debug, Clone, Default)]
pub struct RollingHash {
    a: u32,
    b: u32,
    block_size: usize,
}

impl RollingHash {
    pub fn new(block_size: usize) -> Self {
        Self {
            a: 0,
            b: 0,
            block_size: block_size.max(1),
        }
    }

    /// 用初始数据块计算哈希。
    pub fn init(&mut self, data: &[u8]) -> u32 {
        self.a = 0;
        self.b = 0;
        for (i, byte) in data.iter().enumerate() {
            self.a = (self.a.wrapping_add(*byte as u32)) & 0xFFFF;
            self.b = self.b.wrapping_add((data.len() - i) as u32 * *byte as u32) & 0xFFFF;
        }
        self.digest()
    }

    /// 滚动更新：移除旧字节 `old_byte`，加入新字节 `new_byte`。
    pub fn roll(&mut self, old_byte: u8, new_byte: u8) -> u32 {
        let n = self.block_size as u32;
        self.a = (self
            .a
            .wrapping_sub(old_byte as u32)
            .wrapping_add(new_byte as u32))
            & 0xFFFF;
        self.b = (self
            .b
            .wrapping_sub(n.wrapping_mul(old_byte as u32))
            .wrapping_add(self.a))
            & 0xFFFF;
        self.digest()
    }

    /// 当前哈希摘要。
    pub fn digest(&self) -> u32 {
        (self.b << 16) | self.a
    }
}

/// 强哈希 (FNV-1a 64，mock 实现)。
///
/// 真实实现使用 `sha3::Sha3_256` 取前 8 字节。
pub fn strong_hash(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// 块签名：弱哈希 + 强哈希。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct BlockSignature {
    pub index: usize,
    pub weak_hash: u32,
    pub strong_hash: u64,
}

/// 块增量：目标文件中相对源文件缺失或变化的块。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockDelta {
    /// 在目标文件中的起始块索引。
    pub block_index: usize,
    /// 新块数据 (若与源文件匹配则为空，不会出现在 delta 中)。
    pub data: Vec<u8>,
}

/// 增量同步计算器。
pub struct IncrementalSync {
    block_size: usize,
}

impl IncrementalSync {
    pub fn new(block_size: usize) -> Self {
        Self {
            block_size: block_size.max(64),
        }
    }

    pub fn with_default_block_size() -> Self {
        Self::new(DEFAULT_BLOCK_SIZE)
    }

    pub fn block_size(&self) -> usize {
        self.block_size
    }

    /// 对源文件计算块签名表 (接收端已有数据)。
    pub fn sign(&self, source: &[u8]) -> Vec<BlockSignature> {
        let mut sigs = Vec::new();
        let mut rh = RollingHash::new(self.block_size);
        let mut index = 0usize;
        let mut pos = 0usize;
        while pos < source.len() {
            let end = (pos + self.block_size).min(source.len());
            let block = &source[pos..end];
            let weak = rh.init(block);
            let strong = strong_hash(block);
            sigs.push(BlockSignature {
                index,
                weak_hash: weak,
                strong_hash: strong,
            });
            index += 1;
            pos += self.block_size;
        }
        debug!(
            "incremental sign: {} blocks for {} bytes",
            sigs.len(),
            source.len()
        );
        sigs
    }

    /// 计算从 `source` (已签名) 到 `target` 的块增量。
    ///
    /// 采用块对齐匹配：对 target 的每个块计算弱/强哈希，
    /// 若与某源块签名匹配则跳过，否则加入 delta。
    pub fn diff(&self, source_sigs: &[BlockSignature], target: &[u8]) -> Vec<BlockDelta> {
        let mut sig_map: HashMap<u32, Vec<&BlockSignature>> = HashMap::new();
        for sig in source_sigs {
            sig_map.entry(sig.weak_hash).or_default().push(sig);
        }
        let mut deltas = Vec::new();
        let mut rh = RollingHash::new(self.block_size);
        let mut pos = 0usize;
        let mut block_index = 0usize;
        while pos < target.len() {
            let end = (pos + self.block_size).min(target.len());
            let block = &target[pos..end];
            let weak = rh.init(block);
            let matched = sig_map.get(&weak).and_then(|candidates| {
                let strong = strong_hash(block);
                candidates
                    .iter()
                    .find(|s| s.strong_hash == strong)
                    .map(|s| s.index)
            });
            if matched.is_none() {
                deltas.push(BlockDelta {
                    block_index,
                    data: block.to_vec(),
                });
            }
            block_index += 1;
            pos += self.block_size;
        }
        debug!(
            "incremental diff: {} changed blocks out of {}",
            deltas.len(),
            block_index
        );
        deltas
    }

    /// 应用块增量到源文件，重建目标文件。
    pub fn apply(&self, source: &[u8], deltas: &[BlockDelta]) -> Vec<u8> {
        let mut out = source.to_vec();
        for delta in deltas {
            let start = delta.block_index * self.block_size;
            if start >= out.len() {
                // 扩展输出缓冲区
                if start > out.len() {
                    out.extend(std::iter::repeat_n(0u8, start - out.len()));
                }
                out.extend_from_slice(&delta.data);
            } else {
                let end = (start + delta.data.len()).min(out.len());
                let replace_len = end - start;
                out[start..end].copy_from_slice(&delta.data[..replace_len]);
                if delta.data.len() > replace_len {
                    out.extend_from_slice(&delta.data[replace_len..]);
                }
            }
        }
        out
    }

    /// 压缩数据 (mock：identity；真实实现使用 zstd)。
    pub fn compress(data: &[u8]) -> Vec<u8> {
        // TODO: 替换为 zstd::encode_all
        data.to_vec()
    }

    /// 解压数据 (mock：identity)。
    pub fn decompress(data: &[u8]) -> Vec<u8> {
        data.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rolling_hash_init_deterministic() {
        let mut h1 = RollingHash::new(4);
        let mut h2 = RollingHash::new(4);
        let data = [1u8, 2, 3, 4];
        assert_eq!(h1.init(&data), h2.init(&data));
    }

    #[test]
    fn test_rolling_hash_roll_updates() {
        let block_size = 4;
        let mut h = RollingHash::new(block_size);
        let window = [10u8, 20, 30, 40];
        let init = h.init(&window);
        // 滚动：移除 10，加入 50 -> [20,30,40,50]
        let rolled = h.roll(10, 50);
        // 直接对 [20,30,40,50] init 应得到相同结果
        let mut h2 = RollingHash::new(block_size);
        let direct = h2.init(&[20u8, 30, 40, 50]);
        assert_eq!(rolled, direct);
        assert_ne!(init, rolled);
    }

    #[test]
    fn test_strong_hash_deterministic_and_distinct() {
        let a = strong_hash(b"hello");
        let b = strong_hash(b"hello");
        let c = strong_hash(b"world");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_sign_block_count() {
        let sync = IncrementalSync::new(64);
        // 128 字节 -> 2 个块
        let data = vec![0u8; 128];
        let sigs = sync.sign(&data);
        assert_eq!(sigs.len(), 2);
        assert_eq!(sigs[0].index, 0);
        assert_eq!(sigs[1].index, 1);
    }

    #[test]
    fn test_diff_no_changes() {
        let sync = IncrementalSync::new(64);
        let source = vec![1u8; 128];
        let sigs = sync.sign(&source);
        let deltas = sync.diff(&sigs, &source);
        assert_eq!(deltas.len(), 0);
    }

    #[test]
    fn test_diff_with_changed_block() {
        let sync = IncrementalSync::new(64);
        let source = vec![1u8; 128];
        let sigs = sync.sign(&source);
        // 修改第二个块
        let mut target = source.clone();
        for b in &mut target[64..128] {
            *b = 2;
        }
        let deltas = sync.diff(&sigs, &target);
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].block_index, 1);
        // 应用增量后应重建出 target
        let rebuilt = sync.apply(&source, &deltas);
        assert_eq!(rebuilt, target);
    }

    #[test]
    fn test_apply_reconstructs_target() {
        let sync = IncrementalSync::new(32);
        let source = vec![5u8; 96]; // 3 块
        let mut target = source.clone();
        // 改第一块和第三块
        for b in &mut target[0..32] {
            *b = 9;
        }
        for b in &mut target[64..96] {
            *b = 7;
        }
        let sigs = sync.sign(&source);
        let deltas = sync.diff(&sigs, &target);
        let rebuilt = sync.apply(&source, &deltas);
        assert_eq!(rebuilt, target);
    }

    #[test]
    fn test_compress_decompress_identity() {
        let data = vec![42u8; 100];
        let compressed = IncrementalSync::compress(&data);
        let decompressed = IncrementalSync::decompress(&compressed);
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_block_size_minimum_clamped() {
        let sync = IncrementalSync::new(10); // 小于 64 应被钳制
        assert_eq!(sync.block_size(), 64);
    }
}
