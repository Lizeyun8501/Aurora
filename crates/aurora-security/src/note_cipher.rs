//! 笔记级内容加密（DK-07 S2 — WF-Alpha）。
//!
//! 密钥层级：主 DEK → HKDF-SHA256(note_id) → 每笔记独立 256-bit 密钥。
//! 同一 vault 下笔记间密钥隔离：单笔记密钥泄露不影响其他笔记。
//! 密文格式：`enc1:<hex(nonce || ciphertext+tag)>`（前缀 fail-closed）。
//!
//! 装配：桌面端由 bootstrap 注入 WriteContext.content_cipher；
//! 移动端 S4 接线。无 cipher 环境写加密笔记 = fail-closed 拒绝。

use aurora_core::Error;
use hkdf::Hkdf;
use sha2::Sha256;

use crate::e2ee::{AesGcmCipher, Ciphertext};
use crate::key_hierarchy::WorkspaceDek;

/// HKDF info 域分隔（v1 格式）。
const HKDF_INFO: &[u8] = b"aurora/note-content/v1";
/// 密文前缀（版本化，非此前缀一律拒绝）。
pub const ENC1_PREFIX: &str = "enc1:";

/// 从主 DEK 派生笔记级内容密钥（每笔记独立，跨笔记隔离）。
pub fn derive_note_key(dek: &WorkspaceDek, note_id: &str) -> Result<[u8; 32], Error> {
    let hk = Hkdf::<Sha256>::new(Some(note_id.as_bytes()), dek.as_bytes());
    let mut okm = [0u8; 32];
    hk.expand(HKDF_INFO, &mut okm)
        .map_err(|e| Error::Crypto(format!("note key derive failed: {e}")))?;
    Ok(okm)
}

/// 笔记内容加密器：AES-256-GCM + 每笔记独立密钥（`enc1:` 格式）。
#[derive(Debug, Clone)]
pub struct NoteContentCipher {
    /// 主 DEK（派生源）。
    master: WorkspaceDek,
}

impl NoteContentCipher {
    pub fn new(master: WorkspaceDek) -> Self {
        Self { master }
    }

    /// 加密正文：`enc1:<hex(nonce || ciphertext+tag)>`。
    ///
    /// # Errors
    /// - AES-GCM 加密失败
    pub fn encrypt(&self, note_id: &str, plaintext: &str) -> Result<String, Error> {
        let note_key = derive_note_key(&self.master, note_id)?;
        let dek = WorkspaceDek::from_bytes("note-content", note_key);
        let cipher = AesGcmCipher::new();
        let ct = cipher
            .encrypt_str(plaintext, &dek)
            .map_err(|e| Error::Crypto(format!("note content encrypt: {e}")))?;
        let mut out = String::from(ENC1_PREFIX);
        for b in ct.to_bytes() {
            out.push_str(&format!("{b:02x}"));
        }
        Ok(out)
    }

    /// 解密正文（fail-closed：前缀/长度/认证任何一步失败都不返回明文）。
    ///
    /// # Errors
    /// - 非 `enc1:` 前缀
    /// - hex 解码失败 / 长度不足（< 12B nonce + 16B tag）
    /// - GCM 认证失败（密文或 AAD 篡改）
    pub fn decrypt(&self, note_id: &str, sealed: &str) -> Result<String, Error> {
        let hex_part = sealed
            .strip_prefix(ENC1_PREFIX)
            .ok_or_else(|| Error::Crypto("not an enc1 ciphertext (missing prefix)".into()))?;
        if hex_part.len() % 2 != 0 || hex_part.len() < 2 * (12 + 16) {
            return Err(Error::Crypto("enc1 payload too short or malformed".into()));
        }
        let mut bytes = Vec::with_capacity(hex_part.len() / 2);
        let hexb = hex_part.as_bytes();
        for i in (0..hexb.len()).step_by(2) {
            let hi = (hexb[i] as char)
                .to_digit(16)
                .ok_or_else(|| Error::Crypto("enc1 hex decode failed".into()))?;
            let lo = (hexb[i + 1] as char)
                .to_digit(16)
                .ok_or_else(|| Error::Crypto("enc1 hex decode failed".into()))?;
            bytes.push(((hi << 4) | lo) as u8);
        }
        let note_key = derive_note_key(&self.master, note_id)?;
        let dek = WorkspaceDek::from_bytes("note-content", note_key);
        let ct = Ciphertext::from_bytes(&bytes)
            .map_err(|e| Error::Crypto(format!("enc1 decode: {e}")))?;
        let pt = AesGcmCipher::new()
            .decrypt(&ct, &dek)
            .map_err(|e| Error::Crypto(format!("note content decrypt: {e}")))?;
        Ok(pt.to_string_lossy())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> NoteContentCipher {
        NoteContentCipher::new(WorkspaceDek::new("ws-dk07-s2").unwrap())
    }

    /// roundtrip：加密 → 解密还原明文。
    #[test]
    fn dk07_s2_roundtrip() {
        let c = fixture();
        let sealed = c.encrypt("note-1", "秘密正文 🔐 unicode").unwrap();
        assert!(sealed.starts_with(ENC1_PREFIX));
        assert_eq!(c.decrypt("note-1", &sealed).unwrap(), "秘密正文 🔐 unicode");
    }

    /// 跨笔记密钥隔离：note-1 的密文用 note-2 密钥解 → 拒绝。
    #[test]
    fn dk07_s2_note_key_isolation() {
        let c = fixture();
        let sealed = c.encrypt("note-1", "belongs to note-1").unwrap();
        assert!(c.decrypt("note-2", &sealed).is_err(), "跨笔记解密必须失败");
    }

    /// 前缀 fail-closed：非 enc1 格式一律拒绝（不尝试解析）。
    #[test]
    fn dk07_s2_prefix_fail_closed() {
        let c = fixture();
        assert!(c.decrypt("note-1", "plain text").is_err());
        assert!(c.decrypt("note-1", "enc2:00ff").is_err());
        assert!(c.decrypt("note-1", "enc1:00").is_err(), "长度不足拒绝");
        assert!(c.decrypt("note-1", "enc1:zz").is_err(), "非法 hex 拒绝");
    }

    /// 篡改拒绝（fail-closed，与 dk07_ciphertext_tamper 同语义）。
    #[test]
    fn dk07_s2_tamper_rejected() {
        let c = fixture();
        let mut sealed = c.encrypt("note-1", "integrity matters").unwrap();
        // 翻转密文尾部一个 hex 字符
        let last = sealed.pop().unwrap();
        let flipped = if last == '0' { '1' } else { '0' };
        sealed.push(flipped);
        assert!(c.decrypt("note-1", &sealed).is_err());
    }

    /// 同一 vault 下密文确定性：相同输入两次加密密文不同（随机 nonce）。
    #[test]
    fn dk07_s2_nonce_randomized() {
        let c = fixture();
        let a = c.encrypt("note-1", "same input").unwrap();
        let b = c.encrypt("note-1", "same input").unwrap();
        assert_ne!(a, b, "nonce 必须随机化");
        // 但都可解回同一明文
        assert_eq!(
            c.decrypt("note-1", &a).unwrap(),
            c.decrypt("note-1", &b).unwrap()
        );
    }
}
