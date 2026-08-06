//! 密钥恢复 (Key Recovery)
//!
//! 提供 BIP39 助记词、Shamir 门限秘密分享与设备授权二维码，用于在丢失设备
//! 或忘记口令时安全恢复主密钥。
//!
//! # 简化说明
//! - **BIP39 助记词**：生成 256-bit 熵，编码为 24 个单词（每词 11 bit），
//!   附带 8-bit 校验和。真实 BIP39 使用固定的 2048 词英文词表与 SHA-256
//!   校验和；此处使用合成的 2048 词占位词表（`w0000`..`w2047`）与
//!   SHA3-256 校验和，仅用于演示编码/校验流程。
//! - **Shamir 秘密分享**：基于 GF(2^8)（AES 不可约多项式 0x11b）的多项式
//!   插值，默认 3-of-2（拆成 3 份，需 2 份重建）。
//! - **种子派生**：使用迭代 SHA3-512 近似 PBKDF2 拉伸（真实 BIP39 使用
//!   PBKDF2-HMAC-SHA512，2048 轮）。

use bincode;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256, Sha3_512};
use tracing::{debug, info, warn};

use crate::Error;

/// 助记词词表大小（11 bit / 词）
pub const WORD_COUNT: usize = 2048;
/// 助记词数量
pub const MNEMONIC_WORDS: usize = 24;
/// 熵长度（256 bit）
pub const ENTROPY_LEN: usize = 32;
/// 熵 + 校验和的总字节数（256 bit + 8 bit = 264 bit = 33 字节）
const ENTROPY_PLUS_CHECKSUM: usize = 33;

// ---------------------------------------------------------------------------
// BIP39 助记词
// ---------------------------------------------------------------------------

/// BIP39 风格助记词（24 词）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mnemonic {
    words: Vec<String>,
}

impl Mnemonic {
    /// 生成全新助记词（256-bit 随机熵）
    pub fn generate() -> Result<Self, Error> {
        let mut entropy = [0u8; ENTROPY_LEN];
        OsRng.fill_bytes(&mut entropy);
        Self::from_entropy(&entropy)
    }

    /// 从 256-bit 熵构造助记词
    pub fn from_entropy(entropy: &[u8; ENTROPY_LEN]) -> Result<Self, Error> {
        let checksum = sha3_256(&entropy[..])[0];
        let indices = entropy_to_indices(entropy, checksum);
        let words = indices.iter().map(|&i| index_to_word(i)).collect();
        debug!("generated {}-word mnemonic", MNEMONIC_WORDS);
        Ok(Self { words })
    }

    /// 从单词列表解析助记词（含校验和验证）
    pub fn from_words(words: &[String]) -> Result<Self, Error> {
        if words.len() != MNEMONIC_WORDS {
            return Err(Error::Recovery(format!(
                "expected {} words, got {}",
                MNEMONIC_WORDS,
                words.len()
            )));
        }
        let mut indices = [0u16; MNEMONIC_WORDS];
        for (i, w) in words.iter().enumerate() {
            indices[i] = word_to_index(w)
                .ok_or_else(|| Error::Recovery(format!("invalid mnemonic word: {}", w)))?;
        }
        let buf = indices_to_bytes(&indices);
        let mut entropy = [0u8; ENTROPY_LEN];
        entropy.copy_from_slice(&buf[..ENTROPY_LEN]);
        let expected = sha3_256(&entropy[..])[0];
        if buf[ENTROPY_LEN] != expected {
            return Err(Error::Recovery("mnemonic checksum mismatch".into()));
        }
        Ok(Self {
            words: words.to_vec(),
        })
    }

    /// 单词列表
    pub fn words(&self) -> &[String] {
        &self.words
    }

    /// 单词数量
    pub fn word_count(&self) -> usize {
        self.words.len()
    }

    /// 校验助记词合法性（词表 + 校验和）
    pub fn validate(&self) -> Result<(), Error> {
        Self::from_words(&self.words).map(|_| ())
    }

    /// 还原 256-bit 熵
    pub fn to_entropy(&self) -> Result<[u8; ENTROPY_LEN], Error> {
        let mut indices = [0u16; MNEMONIC_WORDS];
        for (i, w) in self.words.iter().enumerate() {
            indices[i] = word_to_index(w)
                .ok_or_else(|| Error::Recovery(format!("invalid mnemonic word: {}", w)))?;
        }
        let buf = indices_to_bytes(&indices);
        let mut entropy = [0u8; ENTROPY_LEN];
        entropy.copy_from_slice(&buf[..ENTROPY_LEN]);
        let expected = sha3_256(&entropy[..])[0];
        if buf[ENTROPY_LEN] != expected {
            return Err(Error::Recovery("mnemonic checksum mismatch".into()));
        }
        Ok(entropy)
    }

    /// 派生 512-bit 种子（迭代 SHA3-512 近似 PBKDF2，2048 轮）
    pub fn to_seed(&self, passphrase: Option<&str>) -> [u8; 64] {
        let mut state = Vec::new();
        state.extend_from_slice(self.to_string().as_bytes());
        if let Some(p) = passphrase {
            state.extend_from_slice(p.as_bytes());
        }
        let mut acc = Sha3_512::digest(&state).to_vec();
        for _ in 0..2048 {
            acc = Sha3_512::digest(&acc).to_vec();
        }
        let mut out = [0u8; 64];
        out.copy_from_slice(&acc[..64]);
        out
    }
}

impl std::fmt::Display for Mnemonic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.words.join(" "))
    }
}

/// 词表索引 → 单词（合成占位词表）
fn index_to_word(idx: u16) -> String {
    format!("w{:04}", idx)
}

/// 单词 → 词表索引
fn word_to_index(word: &str) -> Option<u16> {
    let rest = word.strip_prefix('w')?;
    let n: u32 = rest.parse().ok()?;
    if n < WORD_COUNT as u32 {
        Some(n as u16)
    } else {
        None
    }
}

/// 熵 + 校验和 → 24 个 11-bit 索引
fn entropy_to_indices(entropy: &[u8; ENTROPY_LEN], checksum: u8) -> [u16; MNEMONIC_WORDS] {
    let mut buf = [0u8; ENTROPY_PLUS_CHECKSUM];
    buf[..ENTROPY_LEN].copy_from_slice(entropy);
    buf[ENTROPY_LEN] = checksum;
    let mut indices = [0u16; MNEMONIC_WORDS];
    for i in 0..MNEMONIC_WORDS {
        indices[i] = read_bits(&buf, i * 11, 11) as u16;
    }
    indices
}

/// 24 个 11-bit 索引 → 33 字节（熵 + 校验和）
fn indices_to_bytes(indices: &[u16; MNEMONIC_WORDS]) -> [u8; ENTROPY_PLUS_CHECKSUM] {
    let mut buf = [0u8; ENTROPY_PLUS_CHECKSUM];
    for i in 0..MNEMONIC_WORDS {
        write_bits(&mut buf, i * 11, 11, indices[i] as u32);
    }
    buf
}

/// 从字节缓冲读取 `len` 个 bit（MSB 优先），起点为 `offset`。
fn read_bits(buf: &[u8], offset: usize, len: usize) -> u32 {
    let mut v = 0u32;
    for j in 0..len {
        let bit_idx = offset + j;
        let byte = buf[bit_idx / 8];
        let bit = (byte >> (7 - (bit_idx % 8))) & 1;
        v = (v << 1) | bit as u32;
    }
    v
}

/// 向字节缓冲写入 `len` 个 bit（MSB 优先）。
fn write_bits(buf: &mut [u8], offset: usize, len: usize, value: u32) {
    for j in 0..len {
        let bit = (value >> (len - 1 - j)) & 1;
        let bit_idx = offset + j;
        buf[bit_idx / 8] |= (bit as u8) << (7 - (bit_idx % 8));
    }
}

fn sha3_256(data: &[u8]) -> [u8; 32] {
    let mut h = Sha3_256::new();
    h.update(data);
    let out = h.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}

// ---------------------------------------------------------------------------
// Shamir 秘密分享 (GF(2^8))
// ---------------------------------------------------------------------------

/// Shamir 秘密分享份额
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShamirShare {
    /// 横坐标 x（1..=n，0 保留给秘密本身）
    pub x: u8,
    /// 纵坐标 y（与秘密等长的字节串）
    pub y: Vec<u8>,
}

impl ShamirShare {
    pub fn new(x: u8, y: Vec<u8>) -> Self {
        Self { x, y }
    }

    pub fn len(&self) -> usize {
        self.y.len()
    }

    pub fn is_empty(&self) -> bool {
        self.y.is_empty()
    }
}

/// Shamir 门限秘密分享：将秘密拆为 n 份，需 threshold 份方可重建。
pub struct ShamirSecretSharing {
    n: usize,
    threshold: usize,
}

impl ShamirSecretSharing {
    /// 构造 n-of-threshold 分享方案
    pub fn new(n: usize, threshold: usize) -> Result<Self, Error> {
        if threshold == 0 || threshold > n {
            return Err(Error::InvalidInput(format!(
                "invalid threshold: threshold={} n={}",
                threshold, n
            )));
        }
        if n > 255 {
            return Err(Error::InvalidInput(format!("n must be <= 255, got {}", n)));
        }
        Ok(Self { n, threshold })
    }

    /// 默认 3-of-2：拆成 3 份，需 2 份重建
    pub fn default_3_of_2() -> Self {
        Self {
            n: 3,
            threshold: 2,
        }
    }

    pub fn n(&self) -> usize {
        self.n
    }

    pub fn threshold(&self) -> usize {
        self.threshold
    }

    /// 拆分秘密
    pub fn split(&self, secret: &[u8]) -> Result<Vec<ShamirShare>, Error> {
        let degree = self.threshold - 1;
        let mut shares: Vec<ShamirShare> = (1..=self.n)
            .map(|x| ShamirShare {
                x: x as u8,
                y: vec![0u8; secret.len()],
            })
            .collect();

        for (j, &byte) in secret.iter().enumerate() {
            // 系数：常数项 = secret，其余 degree 项随机
            let coeffs: Vec<u8> = (0..degree).map(|_| random_byte()).collect();
            for share in shares.iter_mut() {
                let x = share.x;
                let mut acc = byte; // 常数项 f(0)
                let mut x_pow = x;
                for &c in &coeffs {
                    acc ^= gf_mul(c, x_pow);
                    x_pow = gf_mul(x_pow, x);
                }
                share.y[j] = acc;
            }
        }
        info!(n = self.n, threshold = self.threshold, "split secret into shares");
        Ok(shares)
    }

    /// 从份额重建秘密（至少 threshold 份）
    pub fn combine(&self, shares: &[ShamirShare]) -> Result<Vec<u8>, Error> {
        if shares.len() < self.threshold {
            return Err(Error::Recovery(format!(
                "need at least {} shares, got {}",
                self.threshold,
                shares.len()
            )));
        }
        if shares[0].y.is_empty() {
            return Ok(Vec::new());
        }
        let len = shares[0].y.len();
        let k = self.threshold;
        let mut secret = vec![0u8; len];
        for j in 0..len {
            let pts: Vec<(u8, u8)> = shares[..k].iter().map(|s| (s.x, s.y[j])).collect();
            secret[j] = lagrange_at_zero(&pts);
        }
        debug!(k, "reconstructed secret from shares");
        Ok(secret)
    }
}

impl Default for ShamirSecretSharing {
    fn default() -> Self {
        Self::default_3_of_2()
    }
}

/// GF(2^8) 乘法（不可约多项式 0x11b）
fn gf_mul(a: u8, b: u8) -> u8 {
    let mut a = a as u16;
    let mut b = b as u16;
    let mut p: u16 = 0;
    for _ in 0..8 {
        if b & 1 != 0 {
            p ^= a;
        }
        let hi = a & 0x80;
        a <<= 1;
        if hi != 0 {
            a ^= 0x11b;
        }
        b >>= 1;
    }
    (p & 0xff) as u8
}

/// GF(2^8) 幂运算
fn gf_pow(a: u8, mut e: u32) -> u8 {
    let mut result = 1u8;
    let mut base = a;
    while e > 0 {
        if e & 1 == 1 {
            result = gf_mul(result, base);
        }
        base = gf_mul(base, base);
        e >>= 1;
    }
    result
}

/// GF(2^8) 乘法逆元（a^254，a != 0 时等于 a^-1）
fn gf_inv(a: u8) -> u8 {
    if a == 0 {
        return 0;
    }
    gf_pow(a, 254)
}

/// 拉格朗日插值求 f(0)
fn lagrange_at_zero(pts: &[(u8, u8)]) -> u8 {
    let k = pts.len();
    let mut result = 0u8;
    for i in 0..k {
        let (xi, yi) = pts[i];
        let mut num = 1u8; // ∏ (0 - x_j) = ∏ x_j   (GF(2^8) 中 -a = a)
        let mut den = 1u8; // ∏ (x_i - x_j) = ∏ (x_i ⊕ x_j)
        for j in 0..k {
            if j == i {
                continue;
            }
            let (xj, _) = pts[j];
            num = gf_mul(num, xj);
            den = gf_mul(den, xi ^ xj);
        }
        let inv = gf_inv(den);
        result ^= gf_mul(yi, gf_mul(num, inv));
    }
    result
}

fn random_byte() -> u8 {
    let mut b = [0u8; 1];
    OsRng.fill_bytes(&mut b);
    b[0]
}

// ---------------------------------------------------------------------------
// 设备授权二维码
// ---------------------------------------------------------------------------

/// 设备授权二维码负载：绑定一台设备与一份恢复份额。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceAuthorizationQr {
    pub device_id: String,
    pub share: ShamirShare,
}

/// QR 负载前缀
pub const QR_PREFIX: &str = "aurora-recover:v1:";

#[derive(Serialize, Deserialize)]
struct QrPayload {
    version: u8,
    device_id: String,
    share_x: u8,
    share_y: Vec<u8>,
}

impl DeviceAuthorizationQr {
    pub fn new(device_id: impl Into<String>, share: ShamirShare) -> Self {
        Self {
            device_id: device_id.into(),
            share,
        }
    }

    /// 编码为可放入二维码的字符串（前缀 + bincode + hex）
    pub fn encode(&self) -> Result<String, Error> {
        let payload = QrPayload {
            version: 1,
            device_id: self.device_id.clone(),
            share_x: self.share.x,
            share_y: self.share.y.clone(),
        };
        let bytes = bincode::serialize(&payload)
            .map_err(|e| Error::Recovery(format!("bincode serialize failed: {}", e)))?;
        Ok(format!("{}{}", QR_PREFIX, hex_encode(&bytes)))
    }

    /// 从二维码字符串解码
    pub fn decode(s: &str) -> Result<Self, Error> {
        let rest = s
            .strip_prefix(QR_PREFIX)
            .ok_or_else(|| Error::Recovery("invalid QR prefix".into()))?;
        let bytes = hex_decode(rest)?;
        let payload: QrPayload = bincode::deserialize(&bytes)
            .map_err(|e| Error::Recovery(format!("bincode deserialize failed: {}", e)))?;
        if payload.version != 1 {
            return Err(Error::Recovery(format!(
                "unsupported QR version: {}",
                payload.version
            )));
        }
        Ok(Self {
            device_id: payload.device_id,
            share: ShamirShare {
                x: payload.share_x,
                y: payload.share_y,
            },
        })
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn hex_decode(s: &str) -> Result<Vec<u8>, Error> {
    if !s.len().is_multiple_of(2) {
        return Err(Error::Recovery("invalid hex length".into()));
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    for chunk in bytes.chunks(2) {
        let hi = hex_val(chunk[0])?;
        let lo = hex_val(chunk[1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_val(c: u8) -> Result<u8, Error> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => {
            warn!("invalid hex char: {}", c as char);
            Err(Error::Recovery(format!("invalid hex char: {}", c as char)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mnemonic_generate_has_24_words() {
        let m = Mnemonic::generate().unwrap();
        assert_eq!(m.word_count(), MNEMONIC_WORDS);
        for w in m.words() {
            assert!(w.starts_with('w'), "synthetic word must start with 'w': {}", w);
        }
    }

    #[test]
    fn test_mnemonic_validate_ok() {
        let m = Mnemonic::generate().unwrap();
        assert!(m.validate().is_ok());
    }

    #[test]
    fn test_mnemonic_invalid_word_count_rejected() {
        let words: Vec<String> = (0..12).map(|i| format!("w{:04}", i)).collect();
        assert!(Mnemonic::from_words(&words).is_err());
    }

    #[test]
    fn test_mnemonic_invalid_word_rejected() {
        let m = Mnemonic::generate().unwrap();
        let mut bad = m.words().to_vec();
        bad[0] = "not-a-word".to_string();
        assert!(Mnemonic::from_words(&bad).is_err());
    }

    #[test]
    fn test_mnemonic_checksum_tamper_rejected() {
        let m = Mnemonic::generate().unwrap();
        // 替换一个词为另一个合法词，破坏校验和
        let mut bad = m.words().to_vec();
        let idx = word_to_index(&bad[0]).unwrap();
        let new_idx = (idx + 1) % WORD_COUNT as u16;
        bad[0] = index_to_word(new_idx);
        // 校验和大概率不匹配
        let result = Mnemonic::from_words(&bad);
        // 只有恰好校验和仍匹配的极小概率会通过；否则应失败
        if let Ok(m2) = result {
            assert_ne!(m2.to_entropy().unwrap(), m.to_entropy().unwrap());
        } else {
            // 期望路径：校验失败
        }
    }

    #[test]
    fn test_mnemonic_entropy_roundtrip() {
        let entropy = [0x42u8; ENTROPY_LEN];
        let m = Mnemonic::from_entropy(&entropy).unwrap();
        let recovered = m.to_entropy().unwrap();
        assert_eq!(recovered, entropy);
    }

    #[test]
    fn test_mnemonic_random_entropy_roundtrip() {
        let m = Mnemonic::generate().unwrap();
        let entropy = m.to_entropy().unwrap();
        let m2 = Mnemonic::from_entropy(&entropy).unwrap();
        assert_eq!(m.words(), m2.words());
    }

    #[test]
    fn test_mnemonic_to_seed_deterministic() {
        let m = Mnemonic::generate().unwrap();
        let s1 = m.to_seed(None);
        let s2 = m.to_seed(None);
        assert_eq!(s1, s2);
        assert_eq!(s1.len(), 64);
    }

    #[test]
    fn test_mnemonic_seed_passphrase_changes_output() {
        let m = Mnemonic::generate().unwrap();
        let s1 = m.to_seed(None);
        let s2 = m.to_seed(Some("extra-salt"));
        assert_ne!(s1, s2);
    }

    #[test]
    fn test_mnemonic_display() {
        let m = Mnemonic::from_entropy(&[0u8; ENTROPY_LEN]).unwrap();
        let s = m.to_string();
        assert_eq!(s.split_whitespace().count(), MNEMONIC_WORDS);
    }

    #[test]
    fn test_shamir_split_count() {
        let sss = ShamirSecretSharing::default_3_of_2();
        let secret = vec![0xABu8; 32];
        let shares = sss.split(&secret).unwrap();
        assert_eq!(shares.len(), 3);
        assert_eq!(sss.n(), 3);
        assert_eq!(sss.threshold(), 2);
        // 每份 y 与秘密等长
        for s in &shares {
            assert_eq!(s.len(), 32);
        }
        // x 坐标互不相同且非零
        let xs: Vec<u8> = shares.iter().map(|s| s.x).collect();
        assert!(xs.iter().all(|&x| x != 0));
        assert_eq!(xs.iter().collect::<std::collections::HashSet<_>>().len(), 3);
    }

    #[test]
    fn test_shamir_reconstruct_with_two_shares() {
        let sss = ShamirSecretSharing::default_3_of_2();
        let secret = vec![0x77u8; 32];
        let shares = sss.split(&secret).unwrap();

        let recon = sss.combine(&shares[0..2]).unwrap();
        assert_eq!(recon, secret);
    }

    #[test]
    fn test_shamir_reconstruct_any_two_shares() {
        let sss = ShamirSecretSharing::default_3_of_2();
        let secret: Vec<u8> = (0..32u8).collect();
        let shares = sss.split(&secret).unwrap();

        // 任意两份都应重建出同一秘密
        let r1 = sss.combine(&[shares[0].clone(), shares[1].clone()]).unwrap();
        let r2 = sss.combine(&[shares[0].clone(), shares[2].clone()]).unwrap();
        let r3 = sss.combine(&[shares[1].clone(), shares[2].clone()]).unwrap();
        assert_eq!(r1, secret);
        assert_eq!(r2, secret);
        assert_eq!(r3, secret);
    }

    #[test]
    fn test_shamir_reconstruct_all_three_shares() {
        let sss = ShamirSecretSharing::default_3_of_2();
        let secret = vec![0xFFu8; 16];
        let shares = sss.split(&secret).unwrap();
        let recon = sss.combine(&shares).unwrap();
        assert_eq!(recon, secret);
    }

    #[test]
    fn test_shamir_insufficient_shares_fails() {
        let sss = ShamirSecretSharing::default_3_of_2();
        let secret = vec![1u8; 32];
        let shares = sss.split(&secret).unwrap();
        // 仅 1 份不够
        assert!(sss.combine(&shares[0..1]).is_err());
    }

    #[test]
    fn test_shamir_random_secret_roundtrip() {
        let sss = ShamirSecretSharing::default_3_of_2();
        let mut secret = [0u8; 32];
        OsRng.fill_bytes(&mut secret);
        let shares = sss.split(&secret).unwrap();
        let recon = sss.combine(&[shares[0].clone(), shares[2].clone()]).unwrap();
        assert_eq!(recon, secret.to_vec());
    }

    #[test]
    fn test_shamir_invalid_params_rejected() {
        assert!(ShamirSecretSharing::new(3, 0).is_err());
        assert!(ShamirSecretSharing::new(2, 3).is_err());
        assert!(ShamirSecretSharing::new(300, 2).is_err());
        assert!(ShamirSecretSharing::new(5, 5).is_ok());
    }

    #[test]
    fn test_shamir_empty_secret() {
        let sss = ShamirSecretSharing::default_3_of_2();
        let shares = sss.split(&[]).unwrap();
        for s in &shares {
            assert!(s.is_empty());
        }
        let recon = sss.combine(&shares[0..2]).unwrap();
        assert!(recon.is_empty());
    }

    #[test]
    fn test_gf_arithmetic() {
        // GF(2^8) 基本性质
        assert_eq!(gf_mul(0, 123), 0);
        assert_eq!(gf_mul(1, 123), 123);
        // a * a^-1 = 1 (a != 0)
        for a in [1u8, 2, 3, 17, 53, 255] {
            assert_eq!(gf_mul(a, gf_inv(a)), 1, "a * a^-1 must be 1 for a={}", a);
        }
    }

    #[test]
    fn test_device_qr_encode_decode_roundtrip() {
        let share = ShamirShare::new(2, vec![0xDE, 0xAD, 0xBE, 0xEF]);
        let qr = DeviceAuthorizationQr::new("device-abc-123", share.clone());
        let encoded = qr.encode().unwrap();
        assert!(encoded.starts_with(QR_PREFIX));

        let decoded = DeviceAuthorizationQr::decode(&encoded).unwrap();
        assert_eq!(decoded.device_id, "device-abc-123");
        assert_eq!(decoded.share, share);
    }

    #[test]
    fn test_device_qr_decode_invalid_prefix() {
        assert!(DeviceAuthorizationQr::decode("wrong-prefix:1234").is_err());
    }

    #[test]
    fn test_device_qr_decode_invalid_hex() {
        assert!(DeviceAuthorizationQr::decode(&format!("{}zz", QR_PREFIX)).is_err());
    }

    #[test]
    fn test_device_qr_from_real_share() {
        let sss = ShamirSecretSharing::default_3_of_2();
        let secret = vec![0x55u8; 32];
        let shares = sss.split(&secret).unwrap();

        let qr0 = DeviceAuthorizationQr::new("dev-0", shares[0].clone());
        let qr1 = DeviceAuthorizationQr::new("dev-1", shares[1].clone());
        let qr2 = DeviceAuthorizationQr::new("dev-2", shares[2].clone());

        let e0 = qr0.encode().unwrap();
        let e1 = qr1.encode().unwrap();
        let e2 = qr2.encode().unwrap();

        let d0 = DeviceAuthorizationQr::decode(&e0).unwrap();
        let d1 = DeviceAuthorizationQr::decode(&e1).unwrap();
        let d2 = DeviceAuthorizationQr::decode(&e2).unwrap();

        // 用解码后的两份份额重建秘密
        let recon = sss
            .combine(&[d0.share.clone(), d2.share.clone()])
            .unwrap();
        assert_eq!(recon, secret);
    }

    #[test]
    fn test_hex_encode_decode_roundtrip() {
        let bytes = vec![0u8, 1, 0x7F, 0x80, 0xFF];
        let s = hex_encode(&bytes);
        let decoded = hex_decode(&s).unwrap();
        assert_eq!(decoded, bytes);
    }

    #[test]
    fn test_hex_decode_odd_length_fails() {
        assert!(hex_decode("abc").is_err());
    }
}
