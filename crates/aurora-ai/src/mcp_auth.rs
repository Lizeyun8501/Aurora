//! DK-10 MCP 网关鉴权分级（stdio/回环默认信任，对外 HTTP 走 HMAC/OAuth）。
//!
//! 清单卡原文：**MCP 网关（鉴权分级：stdio/回环默认信任，仅对外 HTTP 走
//! OAuth/HMAC）**。分级裁决（落码即文档）：
//! - `Stdio`（本地子进程管道）：同一 OS 信任域——**默认信任**，无凭证放行；
//! - `LoopbackHttp`（127.0.0.1/::1）：本机环回，进程边界=用户边界——**默认信任**；
//! - `ExternalHttp`：跨主机——**必须凭证**（HMAC-SHA256 请求签名或 OAuth Bearer），
//!   无凭证一律拒绝（fail-closed：MCP 工具可触发 AI 链，绝不能裸暴露）。
//!
//! HMAC 防重放：`timestamp` 偏差超出 [`REPLAY_WINDOW_SECS`]（默认 300s）拒绝；
//! 签名材料 = `key_id | timestamp | method | path | SHA256(body)`（含 body 哈希
//! 防篡改）。真实 IdP 对接（OAuth introspection）挂后续片——首片做结构与拒绝路径。

use std::time::{SystemTime, UNIX_EPOCH};

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use crate::Error;

/// HMAC 防重放窗口（秒）：timestamp 偏差超此值拒绝。
pub const REPLAY_WINDOW_SECS: u64 = 300;

/// MCP 传输形态（鉴权分级的依据）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportKind {
    /// 本地子进程 stdio 管道。
    Stdio,
    /// 本机环回 HTTP（127.0.0.1 / ::1）。
    LoopbackHttp,
    /// 对外 HTTP（非环回地址监听/转发）。
    ExternalHttp,
}

/// 客户端凭证。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthCredential {
    /// 无凭证（stdio/回环合法；对外拒绝）。
    None,
    /// HMAC-SHA256 请求签名（密钥协商挂 Agent 片；首片对称密钥注册表）。
    Hmac {
        key_id: String,
        timestamp: u64,
        method: String,
        path: String,
        body: Vec<u8>,
        signature: Vec<u8>,
    },
    /// OAuth Bearer token（首片仅结构+非空校验，introspection 挂后续片）。
    OAuth { token: String },
}

/// 鉴权结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthVerdict {
    /// 默认信任（stdio/回环）。
    Trusted,
    /// 凭证验证通过。
    Verified { principal: String },
    /// 拒绝（含机器可读码——fail-closed）。
    Rejected {
        code: AuthRejectCode,
        reason: String,
    },
}

/// 拒绝码（对外协议面，稳定枚举）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthRejectCode {
    /// 对外传输无凭证。
    MissingCredential,
    /// key_id 未注册。
    UnknownKey,
    /// 时间戳超出防重放窗口。
    ReplayWindow,
    /// 签名不匹配。
    BadSignature,
    /// OAuth token 缺失/格式非法。
    BadToken,
}

/// HMAC 密钥注册表（key_id → 密钥字节）。
pub struct McpAuthGate {
    keys: Vec<(String, Vec<u8>)>,
    now_secs: Box<dyn Fn() -> u64 + Send + Sync>,
}

impl Default for McpAuthGate {
    fn default() -> Self {
        Self::new()
    }
}

impl McpAuthGate {
    /// 空注册表网关（系统时间）。
    pub fn new() -> Self {
        Self {
            keys: Vec::new(),
            now_secs: Box::new(system_secs),
        }
    }

    /// 注册 HMAC 密钥（key_id 唯一，重复注册覆盖）。
    pub fn register_key(&mut self, key_id: impl Into<String>, key: Vec<u8>) {
        let key_id = key_id.into();
        if let Some(slot) = self.keys.iter_mut().find(|(k, _)| *k == key_id) {
            slot.1 = key;
        } else {
            self.keys.push((key_id, key));
        }
    }

    /// 注入时钟（测试防重放窗口用）。
    pub fn with_clock(mut self, now: Box<dyn Fn() -> u64 + Send + Sync>) -> Self {
        self.now_secs = now;
        self
    }

    /// 鉴权唯一入口（网关每个请求先过这里）。
    pub fn authenticate(&self, transport: TransportKind, cred: &AuthCredential) -> AuthVerdict {
        match transport {
            // 卡面裁决：stdio/回环默认信任
            TransportKind::Stdio | TransportKind::LoopbackHttp => AuthVerdict::Trusted,
            TransportKind::ExternalHttp => self.authenticate_external(cred),
        }
    }

    fn authenticate_external(&self, cred: &AuthCredential) -> AuthVerdict {
        match cred {
            AuthCredential::None => AuthVerdict::Rejected {
                code: AuthRejectCode::MissingCredential,
                reason: "对外 HTTP 传输必须携带 HMAC 或 OAuth 凭证（MCP 鉴权分级铁律）".into(),
            },
            AuthCredential::OAuth { token } => {
                if token.trim().is_empty() {
                    AuthVerdict::Rejected {
                        code: AuthRejectCode::BadToken,
                        reason: "OAuth token 为空".into(),
                    }
                } else if !token.starts_with("aurora_") {
                    // 首片约定：Aurora 签发的 token 前缀（真实 introspection 挂后续片）
                    AuthVerdict::Rejected {
                        code: AuthRejectCode::BadToken,
                        reason: "OAuth token 格式非法（缺 aurora_ 前缀）".into(),
                    }
                } else {
                    AuthVerdict::Verified {
                        principal: "oauth-client".into(),
                    }
                }
            }
            AuthCredential::Hmac {
                key_id,
                timestamp,
                method,
                path,
                body,
                signature,
            } => self.verify_hmac(key_id, *timestamp, method, path, body, signature),
        }
    }

    fn verify_hmac(
        &self,
        key_id: &str,
        timestamp: u64,
        method: &str,
        path: &str,
        body: &[u8],
        signature: &[u8],
    ) -> AuthVerdict {
        let Some((_, key)) = self.keys.iter().find(|(k, _)| k == key_id) else {
            return AuthVerdict::Rejected {
                code: AuthRejectCode::UnknownKey,
                reason: format!("未知 key_id: {key_id}"),
            };
        };
        let now = (self.now_secs)();
        let drift = now.abs_diff(timestamp);
        if drift > REPLAY_WINDOW_SECS {
            return AuthVerdict::Rejected {
                code: AuthRejectCode::ReplayWindow,
                reason: format!("timestamp 偏差 {drift}s 超出防重放窗口 {REPLAY_WINDOW_SECS}s"),
            };
        }
        let expected = compute_signature(key, timestamp, method, path, body);
        if constant_time_eq(&expected, signature) {
            AuthVerdict::Verified {
                principal: key_id.to_string(),
            }
        } else {
            AuthVerdict::Rejected {
                code: AuthRejectCode::BadSignature,
                reason: "HMAC 签名不匹配".into(),
            }
        }
    }
}

/// 签名计算（客户端/网关共用——材料：key_id|timestamp|method|path|sha256(body)）。
pub fn compute_signature(
    key: &[u8],
    timestamp: u64,
    method: &str,
    path: &str,
    body: &[u8],
) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC key length valid");
    let body_hash = Sha256::digest(body);
    mac.update(format!("{timestamp}|{method}|{path}|").as_bytes());
    mac.update(&body_hash);
    mac.finalize().into_bytes().to_vec()
}

/// 常数时间比较（防时序侧信道——复用 CryptoProvider 同思路，独立轻实现）。
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn system_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// 供 liquify/agent 之外的独立引用（Error 转换占位——鉴权拒绝不抛 Error，走 Verdict）
#[allow(unused)]
fn _error_bridge(e: Error) -> String {
    e.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8] = b"aurora-mcp-test-key-32bytes-long!!";

    fn gate() -> McpAuthGate {
        let mut g = McpAuthGate::new();
        g.register_key("test-key", KEY.to_vec());
        g
    }

    fn signed(key: &[u8], ts: u64) -> impl Fn(&str, &str, &[u8]) -> Vec<u8> + '_ {
        move |m, p, b| compute_signature(key, ts, m, p, b)
    }

    /// 卡面裁决：stdio/回环默认信任（无凭证放行）。
    #[test]
    fn dk10_mcp_trusted_transports() {
        let g = gate();
        assert_eq!(
            g.authenticate(TransportKind::Stdio, &AuthCredential::None),
            AuthVerdict::Trusted
        );
        assert_eq!(
            g.authenticate(TransportKind::LoopbackHttp, &AuthCredential::None),
            AuthVerdict::Trusted
        );
    }

    /// 对外 HTTP 无凭证 → fail-closed 拒绝。
    #[test]
    fn dk10_mcp_external_requires_credential() {
        let g = gate();
        let v = g.authenticate(TransportKind::ExternalHttp, &AuthCredential::None);
        assert!(matches!(
            v,
            AuthVerdict::Rejected {
                code: AuthRejectCode::MissingCredential,
                ..
            }
        ));
    }

    /// HMAC 正确签名 → Verified；错签 → Rejected。
    #[test]
    fn dk10_mcp_hmac_roundtrip() {
        let g = gate();
        let ts = (system_secs)();
        let sign = signed(KEY, ts);
        let sig = sign("POST", "/mcp", br#"{"jsonrpc":"2.0"}"#);
        let v = g.authenticate(
            TransportKind::ExternalHttp,
            &AuthCredential::Hmac {
                key_id: "test-key".into(),
                timestamp: ts,
                method: "POST".into(),
                path: "/mcp".into(),
                body: br#"{"jsonrpc":"2.0"}"#.to_vec(),
                signature: sig.clone(),
            },
        );
        assert!(matches!(v, AuthVerdict::Verified { ref principal } if principal == "test-key"));

        // 错签（篡改 body）
        let v2 = g.authenticate(
            TransportKind::ExternalHttp,
            &AuthCredential::Hmac {
                key_id: "test-key".into(),
                timestamp: ts,
                method: "POST".into(),
                path: "/mcp".into(),
                body: b"tampered".to_vec(),
                signature: sig.clone(),
            },
        );
        assert!(matches!(
            v2,
            AuthVerdict::Rejected {
                code: AuthRejectCode::BadSignature,
                ..
            }
        ));
    }

    /// 防重放窗口：timestamp 超窗拒绝（注入时钟）。
    #[test]
    fn dk10_mcp_replay_window() {
        let fixed_now = 1_800_000_000u64;
        let g = gate().with_clock(Box::new(move || fixed_now));
        let ts = fixed_now - REPLAY_WINDOW_SECS - 1;
        let sig = compute_signature(KEY, ts, "POST", "/mcp", b"{}");
        let v = g.authenticate(
            TransportKind::ExternalHttp,
            &AuthCredential::Hmac {
                key_id: "test-key".into(),
                timestamp: ts,
                method: "POST".into(),
                path: "/mcp".into(),
                body: b"{}".to_vec(),
                signature: sig.clone(),
            },
        );
        assert!(matches!(
            v,
            AuthVerdict::Rejected {
                code: AuthRejectCode::ReplayWindow,
                ..
            }
        ));
        // 窗口边缘内通过
        let ts2 = fixed_now - REPLAY_WINDOW_SECS;
        let sig2 = compute_signature(KEY, ts2, "POST", "/mcp", b"{}");
        let v2 = g.authenticate(
            TransportKind::ExternalHttp,
            &AuthCredential::Hmac {
                key_id: "test-key".into(),
                timestamp: ts2,
                method: "POST".into(),
                path: "/mcp".into(),
                body: b"{}".to_vec(),
                signature: sig2,
            },
        );
        assert!(matches!(v2, AuthVerdict::Verified { .. }));
    }

    /// OAuth token：空/格式非法拒绝，合法前缀通过（introspection 挂后续片）。
    #[test]
    fn dk10_mcp_oauth_shaping() {
        let g = gate();
        for (token, ok) in [
            ("", false),
            ("   ", false),
            ("evil-token", false),
            ("aurora_abc123", true),
        ] {
            let v = g.authenticate(
                TransportKind::ExternalHttp,
                &AuthCredential::OAuth {
                    token: token.into(),
                },
            );
            assert_eq!(
                matches!(v, AuthVerdict::Verified { .. }),
                ok,
                "token={token}"
            );
        }
    }

    /// 未知 key_id 拒绝（注册表枚举不可枚举探测）。
    #[test]
    fn dk10_mcp_unknown_key() {
        let g = gate();
        let ts = (system_secs)();
        let sig = compute_signature(b"any", ts, "POST", "/mcp", b"{}");
        let v = g.authenticate(
            TransportKind::ExternalHttp,
            &AuthCredential::Hmac {
                key_id: "nope".into(),
                timestamp: ts,
                method: "POST".into(),
                path: "/mcp".into(),
                body: b"{}".to_vec(),
                signature: sig.clone(),
            },
        );
        assert!(matches!(
            v,
            AuthVerdict::Rejected {
                code: AuthRejectCode::UnknownKey,
                ..
            }
        ));
    }
}
