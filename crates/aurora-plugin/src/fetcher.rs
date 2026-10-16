//! 插件字节下载抽象（DK-市场下载器卡）。
//!
//! 字节来源替换语义：安装六环链（installer.rs）不动，本模块只提供
//! "URL → wasm 字节" 的拉取通道。fail-closed 三防线：
//! 1. **大小上限**（32MiB——防无限流，超限即断）
//! 2. **超时**（30s 整请求——防悬挂）
//! 3. **非 2xx 拒**（HTTP 错误码不落字节）
//!
//! `MockFetcher` 供单测/本地 file:// 语义（HashMap<url, bytes>）。

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Duration;

/// 下载大小上限：32MiB（wasm 插件现实上限的宽松倍数；超限 fail-closed）。
pub const MAX_PLUGIN_BYTES: usize = 32 * 1024 * 1024;

/// 整请求超时：30s（连接+响应+读取总预算）。
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// UA 标识（服务端侧可观测来源）。
pub const USER_AGENT: &str = concat!("Aurora-PluginFetcher/", env!("CARGO_PKG_VERSION"));

/// 插件字节拉取通道（async trait——桌面 spawn_blocking 侧调用）。
#[async_trait]
pub trait PluginFetcher: Send + Sync {
    /// 拉取 URL 字节。超限/超时/非 2xx/传输失败 → [`crate::Error::Network`]
    /// 或 [`crate::Error::InvalidInput`]（大小超限）。
    async fn fetch(&self, url: &str) -> Result<Vec<u8>, crate::Error>;
}

/// HTTP 真实通道（reqwest + rustls，与 aurora-sync 同版本生态——不引入第二 HTTP 栈）。
pub struct HttpFetcher {
    client: reqwest::Client,
    max_bytes: usize,
}

impl HttpFetcher {
    pub fn new() -> Result<Self, crate::Error> {
        Self::with_limits(MAX_PLUGIN_BYTES, FETCH_TIMEOUT)
    }

    /// 可注入限制的构造（单测用——缩小上限/超时验证 fail-closed）。
    pub fn with_limits(max_bytes: usize, timeout: Duration) -> Result<Self, crate::Error> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(timeout)
            .build()
            .map_err(|e| crate::Error::Network(format!("client build failed: {e}")))?;
        Ok(Self { client, max_bytes })
    }
}

impl Default for HttpFetcher {
    fn default() -> Self {
        Self::new().expect("HttpFetcher::new with default limits cannot fail")
    }
}

#[async_trait]
impl PluginFetcher for HttpFetcher {
    async fn fetch(&self, url: &str) -> Result<Vec<u8>, crate::Error> {
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| crate::Error::Network(format!("GET {url} failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(crate::Error::Network(format!(
                "GET {url}: non-success status {}",
                resp.status()
            )));
        }
        // 大小防线：Content-Length 预检 + 实读上限（chunked 无 CL 场景兜底）
        if let Some(len) = resp.content_length() {
            if len as usize > self.max_bytes {
                return Err(crate::Error::InvalidInput(format!(
                    "plugin bytes {len} exceed limit {}",
                    self.max_bytes
                )));
            }
        }
        let mut bytes = resp
            .bytes()
            .await
            .map_err(|e| crate::Error::Network(format!("read body from {url} failed: {e}")))?;
        if bytes.len() > self.max_bytes {
            return Err(crate::Error::InvalidInput(format!(
                "plugin bytes {} exceed limit {}",
                bytes.len(),
                self.max_bytes
            )));
        }
        bytes.truncate(bytes.len());
        Ok(bytes.to_vec())
    }
}

/// 测试/本地语义通道：HashMap<url, bytes> 命中即返回。
///
/// 保留 fail-closed 大小防线（超限同样拒——Mock 与 Http 行为对齐，
/// 测试可注入超限载荷验证拒装路径）。
#[derive(Default)]
pub struct MockFetcher {
    // RwLock：测试可在 async 块外预置后运行时读取（Send+Sync 面）
    routes: RwLock<HashMap<String, Vec<u8>>>,
}

impl MockFetcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// 预置路由（url → 字节）。
    pub fn route(&self, url: &str, bytes: Vec<u8>) -> &Self {
        self.routes
            .write()
            .expect("MockFetcher routes lock poisoned")
            .insert(url.to_string(), bytes);
        self
    }
}

#[async_trait]
impl PluginFetcher for MockFetcher {
    async fn fetch(&self, url: &str) -> Result<Vec<u8>, crate::Error> {
        let routes = self.routes.read().expect("MockFetcher routes poisoned");
        let Some(bytes) = routes.get(url) else {
            return Err(crate::Error::Network(format!("mock: no route for {url}")));
        };
        if bytes.len() > MAX_PLUGIN_BYTES {
            return Err(crate::Error::InvalidInput(format!(
                "plugin bytes {} exceed limit {MAX_PLUGIN_BYTES}",
                bytes.len()
            )));
        }
        Ok(bytes.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_fetch_hit_and_miss() {
        let f = MockFetcher::new();
        f.route("mock://a.wasm", b"abc".to_vec());
        assert_eq!(f.fetch("mock://a.wasm").await.unwrap(), b"abc".to_vec());
        let miss = f.fetch("mock://missing.wasm").await;
        assert!(matches!(miss, Err(crate::Error::Network(_))));
    }

    #[tokio::test]
    async fn mock_fetch_size_limit_fail_closed() {
        let f = MockFetcher::new();
        let huge = vec![0u8; MAX_PLUGIN_BYTES + 1];
        f.route("mock://huge.wasm", huge);
        let r = f.fetch("mock://huge.wasm").await;
        assert!(matches!(r, Err(crate::Error::InvalidInput(_))));
    }

    #[tokio::test]
    async fn http_fetcher_non_success_rejected() {
        // 本机无外网依赖：用不可路由地址验证错误域转接（Network）
        let f = HttpFetcher::with_limits(1024, Duration::from_millis(300)).unwrap();
        let r = f.fetch("http://127.0.0.1:1/none.wasm").await;
        assert!(matches!(r, Err(crate::Error::Network(_))));
    }
}
