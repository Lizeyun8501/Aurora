//! DK-2.9v2-a: Web Clipper localhost 桥（批复方案 A——2026-10-18）。
//!
//! 浏览器扩展（2.9v2-b MV3，后行）经 `POST http://127.0.0.1:{port}/capture`
//! 推送网页捕获内容——desktop 侧起桥接服务，收包后走 SharePack import 全纪律
//! 建笔记。
//!
//! # 安全线（批复卡原文兑现）
//! - **token 鉴权**：desktop 生成随机 token，扩展配置一次；每请求校验
//!   `Authorization: Bearer {token}`——不匹配 401（恒定耗时比较防时序侧信道）；
//! - **只写不读**：协议仅两条路——`POST /capture`（写）与 `OPTIONS /capture`
//!   （CORS 预检，零数据面）；**无任何 GET**——扩展无法借此拉取库内数据；
//! - **仅 localhost**：bind 127.0.0.1，外部网络不可达。
//!
//! # 诚实边界
//! - 捕获包复用 SharePack schema（title/content/createdAt）——来源 url 以
//!   引用行前缀拼入 content（schema 零变更，v1 简化）；
//! - HTTP 解析为 minimal 实现（仅本协议所需子集：请求行/Content-Length/
//!   Authorization 三头）——非通用服务器，超出子集即 400。

use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// 请求体上限（1 MB——网页文本量级宽裕，防滥用）。
pub const MAX_BODY_LEN: usize = 1024 * 1024;

/// 桥接捕获请求载荷（扩展 POST body JSON）。
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CaptureRequest {
    pub title: String,
    pub url: String,
    /// 网页正文（扩展侧提取的 markdown/纯文本）。
    pub content: String,
}

/// HTTP 处理结果（desktop 壳据此落 import 链）。
#[derive(Debug, PartialEq)]
pub struct CaptureOutcome {
    pub status: u16,
    /// status=201 时承载转换好的捕获载荷（壳层负责建笔记）。
    pub payload: Option<CaptureRequest>,
}

/// 恒定耗时 token 比较（防时序侧信道——sha256 后比较定长摘要）。
fn token_matches(provided: &str, expected: &str) -> bool {
    let a = Sha256::digest(provided.as_bytes());
    let b = Sha256::digest(expected.as_bytes());
    a.as_slice() == b.as_slice()
}

/// 从请求头块提取 `Authorization: Bearer {token}`。
fn extract_bearer(head: &str) -> Option<&str> {
    head.lines()
        .find(|l| l.to_ascii_lowercase().starts_with("authorization:"))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|t| t.trim())
}

/// 从头块提取 Content-Length（缺省 0）。
fn extract_content_length(head: &str) -> usize {
    head.lines()
        .find(|l| l.to_ascii_lowercase().starts_with("content-length:"))
        .and_then(|l| l.split_once(':'))
        .and_then(|(_, v)| v.trim().parse().ok())
        .unwrap_or(0)
}

/// minimal HTTP 请求解析：返回 (method, path, head, body)。
///
/// 只支持本协议子集——请求行 + 头块 + Content-Length body；其余一律 Err。
fn parse_request(buf: &[u8]) -> Result<(String, String, String, Vec<u8>), String> {
    let sep = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| "malformed request: no header terminator".to_string())?;
    let head = std::str::from_utf8(&buf[..sep])
        .map_err(|_| "malformed request: non-utf8 header".to_string())?
        .to_string();
    let body = buf[sep + 4..].to_vec();
    let mut lines = head.lines();
    let request_line = lines.next().ok_or("empty request")?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().ok_or("no method")?.to_string();
    let path = parts.next().ok_or("no path")?.to_string();
    Ok((method, path, head, body))
}

/// 协议路由：校验 token → 校验路径/方法 → 解析载荷。
///
/// `token` 为空视为未配置（桥未就绪）——一切请求 401。
pub fn route(method: &str, path: &str, head: &str, body: &[u8], token: &str) -> CaptureOutcome {
    // CORS 预检：零数据面，放行（token 在真正的 POST 上校验）
    if method == "OPTIONS" {
        return CaptureOutcome {
            status: 204,
            payload: None,
        };
    }
    if token.is_empty() {
        return CaptureOutcome {
            status: 401,
            payload: None,
        };
    }
    let Some(provided) = extract_bearer(head) else {
        return CaptureOutcome {
            status: 401,
            payload: None,
        };
    };
    if !token_matches(provided, token) {
        return CaptureOutcome {
            status: 401,
            payload: None,
        };
    }
    if method != "POST" || path != "/capture" {
        return CaptureOutcome {
            status: 404,
            payload: None,
        };
    }
    if body.len() > MAX_BODY_LEN {
        return CaptureOutcome {
            status: 413,
            payload: None,
        };
    }
    match serde_json::from_slice::<CaptureRequest>(body) {
        Ok(req) => CaptureOutcome {
            status: 201,
            payload: Some(req),
        },
        Err(_) => CaptureOutcome {
            status: 400,
            payload: None,
        },
    }
}

/// HTTP 响应字节（minimal——状态行+CORS 头+定长 body）。
pub fn render_response(status: u16, body: &str) -> Vec<u8> {
    let reason = match status {
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        413 => "Payload Too Large",
        _ => "Internal Server Error",
    };
    let cors = if status == 204 {
        String::from(
            "Access-Control-Allow-Origin: *\r\n\
             Access-Control-Allow-Methods: POST, OPTIONS\r\n\
             Access-Control-Allow-Headers: Authorization, Content-Type\r\n\
             Access-Control-Max-Age: 86400\r\n",
        )
    } else {
        String::from("Access-Control-Allow-Origin: *\r\n")
    };
    format!(
        "HTTP/1.1 {status} {reason}\r\n{cors}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

/// 桥主循环：accept → 读请求（Content-Length 定长）→ route → 响应。
///
/// 单连接串行处理（捕获频率低——无并发需求，简化资源面）。
pub async fn run_bridge(
    listener: TcpListener,
    token: String,
    sink: tokio::sync::mpsc::UnboundedSender<CaptureRequest>,
) -> Result<(), String> {
    loop {
        let (mut stream, _) = listener
            .accept()
            .await
            .map_err(|e| format!("bridge accept failed: {e}"))?;
        // 读到头终结符 → 补读 body（Content-Length）
        let mut buf: Vec<u8> = Vec::with_capacity(4096);
        let mut chunk = [0u8; 4096];
        loop {
            let n = stream
                .read(&mut chunk)
                .await
                .map_err(|e| format!("bridge read failed: {e}"))?;
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            let head_end = buf.windows(4).position(|w| w == b"\r\n\r\n");
            if let Some(sep) = head_end {
                let head = String::from_utf8_lossy(&buf[..sep]).to_string();
                let need = extract_content_length(&head);
                if buf.len() >= sep + 4 + need || buf.len() > MAX_BODY_LEN + 8192 {
                    break;
                }
            }
        }
        let outcome = match parse_request(&buf) {
            Ok((method, path, head, body)) => route(&method, &path, &head, &body, &token),
            Err(_) => CaptureOutcome {
                status: 400,
                payload: None,
            },
        };
        let resp = match &outcome.payload {
            Some(req) => {
                let _ = sink.send(req.clone());
                render_response(outcome.status, "{\"ok\":true}")
            }
            None => render_response(outcome.status, ""),
        };
        let _ = stream.write_all(&resp).await;
        let _ = stream.flush().await;
        // Connection: close——句柄随作用域释放
    }
}

/// 生成随机 token（32 hex——复用 sha2：熵源由宿主提供，此处做确定性编码
/// 展示；真随机性由 desktop 侧 rand 面保证）。
pub fn format_token(entropy: &[u8]) -> String {
    let h = Sha256::digest(entropy);
    h.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod capture_tests {
    use super::*;

    const TOKEN: &str = "test-token-0123456789abcdef";

    fn post(path: &str, token_header: Option<&str>, body: &[u8]) -> CaptureOutcome {
        let mut head = format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n",
            body.len()
        );
        if let Some(t) = token_header {
            head.push_str(&format!("Authorization: Bearer {t}\r\n"));
        }
        head.push_str("\r\n");
        let mut buf = head.into_bytes();
        buf.extend_from_slice(body);
        let (m, p, h, b) = parse_request(&buf).unwrap();
        route(&m, &p, &h, &b, TOKEN)
    }

    #[test]
    fn dk29a_capture_201_with_valid_token() {
        let body =
            r##"{"title":"网页标题","url":"https://example.com/a","content":"# 正文"}"##.as_bytes();
        let out = post("/capture", Some(TOKEN), body);
        assert_eq!(out.status, 201);
        let req = out.payload.expect("201 须有载荷");
        assert_eq!(req.title, "网页标题");
        assert_eq!(req.url, "https://example.com/a");
    }

    #[test]
    fn dk29a_bad_token_401() {
        let body = br#"{"title":"t","url":"u","content":"c"}"#;
        assert_eq!(post("/capture", Some("wrong"), body).status, 401);
        assert_eq!(post("/capture", None, body).status, 401);
        // 空 token（未配置）恒 401——即使头里带了什么
        let head = format!(
            "POST /capture HTTP/1.1\r\nContent-Length: {}\r\nAuthorization: Bearer {}\r\n\r\n",
            body.len(),
            TOKEN
        );
        let mut buf = head.clone().into_bytes();
        buf.extend_from_slice(body);
        let (m, p, h, b) = parse_request(&buf).unwrap();
        assert_eq!(route(&m, &p, &h, &b, "").status, 401);
    }

    #[test]
    fn dk29a_no_read_surface() {
        // GET 任何路径（带合法 token）——404（只写协议，零读取面）
        let head = format!("GET /capture HTTP/1.1\r\nAuthorization: Bearer {TOKEN}\r\n\r\n");
        let (m, p, h, b) = parse_request(&head.into_bytes()).unwrap();
        assert_eq!(route(&m, &p, &h, &b, TOKEN).status, 404);
        let head = format!("GET / HTTP/1.1\r\nAuthorization: Bearer {TOKEN}\r\n\r\n");
        let (m, p, h, b) = parse_request(&head.into_bytes()).unwrap();
        assert_eq!(route(&m, &p, &h, &b, TOKEN).status, 404);
    }

    #[test]
    fn dk29a_options_preflight_zero_data() {
        let head = "OPTIONS /capture HTTP/1.1\r\nOrigin: chrome-extension://abc\r\n\r\n";
        let (m, p, h, b) = parse_request(head.as_bytes()).unwrap();
        let out = route(&m, &p, &h, &b, TOKEN);
        assert_eq!(out.status, 204);
        assert!(out.payload.is_none(), "预检零数据面");
        let resp = render_response(204, "");
        assert!(String::from_utf8_lossy(&resp).contains("Access-Control-Allow-Origin"));
    }

    #[test]
    fn dk29a_bad_json_400_and_oversize_413() {
        assert_eq!(post("/capture", Some(TOKEN), b"not json").status, 400);
        let big = vec![b'x'; MAX_BODY_LEN + 1];
        assert_eq!(post("/capture", Some(TOKEN), &big).status, 413);
    }

    /// 行为级集成：真 bind + 真 HTTP POST（tokio 测试 runtime）——
    /// 全链（连接→解析→路由→sink 收包→响应）验证。
    #[tokio::test]
    async fn dk29a_bridge_end_to_end() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral");
        let port = listener.local_addr().unwrap().port();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let handle = tokio::spawn(run_bridge(listener, TOKEN.into(), tx));

        // 真 TCP 客户端 POST
        let mut conn = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect bridge");
        let body = r##"{"title":"e2e 捕获","url":"https://x.y/z","content":"内容"}"##.as_bytes();
        let req = format!(
            "POST /capture HTTP/1.1\r\nAuthorization: Bearer {TOKEN}\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        let mut wire = req.into_bytes();
        wire.extend_from_slice(body);
        conn.write_all(&wire).await.unwrap();
        let mut resp_buf = vec![0u8; 1024];
        let n = conn.read(&mut resp_buf).await.unwrap();
        assert!(String::from_utf8_lossy(&resp_buf[..n]).starts_with("HTTP/1.1 201"));

        // sink 收到载荷
        let got = rx.recv().await.expect("sink 收包");
        assert_eq!(got.title, "e2e 捕获");
        assert_eq!(got.url, "https://x.y/z");

        handle.abort();
    }

    #[test]
    fn dk29a_token_format() {
        let t = format_token(b"entropy");
        assert_eq!(t.len(), 64);
        assert_eq!(format_token(b"entropy"), t); // 确定性
    }
}
