//! AppCore — 核心应用聚合根（V19 §36.1）
//!
//! `AppCore` 是 V19 架构设计的入口对象，聚合 7 大 Trait 实现 + 事件总线，
//! 作为依赖注入容器被 Tauri / Capacitor / WASM 平台适配层引用。
//!
//! # 设计原则（V19 §27.2）
//!
//! - 核心 crate 不依赖任何平台 SDK（Tauri/Capacitor），仅定义 Trait 接口；
//! - 平台适配在 `src-tauri` 和 `crates/mobile-ffi` 中实现 Trait 并注入 `AppCore`；
//! - Tauri command 和 UniFFI wrapper 保持薄层，仅做参数接收与错误转换。
//!
//! # 启动流程
//!
//! ```text
//! 1. 平台层构造 Trait 实现（DesktopPlatform / MobilePlatform / WebPlatform）
//! 2. AppCore::builder() 注入各实现
//! 3. app_core.startup()  → replay_unconsumed()  + 健康检查 + 索引校验
//! 4. app_core.run()      → 启动 EventBus 消费循环
//! ```

use std::sync::Arc;

use crate::event_bus::layered::LayeredEventBus;
use crate::traits::*;
use crate::Error;
use tracing::{error, info, warn};

/// 核心应用聚合根（V19 §36.1 `AppCore`）。
pub struct AppCore {
    /// 同步目标（iroh P2P / WebDAV / S3）。
    pub sync_target: Arc<dyn sync_target::SyncTarget>,
    /// 密码学服务（跨层服务，AES-256-GCM + Argon2id + ML-KEM-768）。
    pub crypto: Arc<dyn crypto_provider::CryptoProvider>,
    /// AI 推理服务（本地 / 云端）。
    pub ai: Arc<dyn ai_provider::AIProvider>,
    /// 键值存储（配置 / 缓存 / 索引元数据）。
    pub kv_store: Arc<dyn kv_store::KVStore>,
    /// 搜索后端（Tantivy / SQLite FTS5）。
    pub search: Arc<dyn search_backend::SearchBackend>,
    /// OCR 引擎（PaddleOCR / Tesseract）。
    pub ocr: Arc<dyn ocr_provider::OcrProvider>,
    /// 插件运行时（Wasmtime + WASI）。
    pub plugin: Arc<dyn plugin_runtime::PluginRuntime>,
    /// 分层事件总线（High/Medium/Low 三通道）。
    pub event_bus: Arc<LayeredEventBus>,
    /// 已注册投影（V20 §4.5 读模型; 启动时 catch_up 增量追赶）。
    projections: Vec<Arc<dyn crate::event_bus::projection::Projection>>,
    /// FSRS 复习队列（V20 Phase 3 Distill — 今日页「复习」分区）。
    pub review_queue: Arc<crate::l3_domain::fsrs::ReviewQueue>,
}

/// AppCore 构建器（依赖注入容器）。
pub struct AppCoreBuilder {
    sync_target: Option<Arc<dyn sync_target::SyncTarget>>,
    crypto: Option<Arc<dyn crypto_provider::CryptoProvider>>,
    ai: Option<Arc<dyn ai_provider::AIProvider>>,
    kv_store: Option<Arc<dyn kv_store::KVStore>>,
    search: Option<Arc<dyn search_backend::SearchBackend>>,
    ocr: Option<Arc<dyn ocr_provider::OcrProvider>>,
    plugin: Option<Arc<dyn plugin_runtime::PluginRuntime>>,
    event_bus_store: Option<Arc<dyn crate::event_bus::layered::EventQueueStore>>,
    projections: Vec<Arc<dyn crate::event_bus::projection::Projection>>,
}

impl AppCoreBuilder {
    /// 创建空构建器。
    pub fn new() -> Self {
        Self {
            sync_target: None,
            crypto: None,
            ai: None,
            kv_store: None,
            search: None,
            ocr: None,
            plugin: None,
            event_bus_store: None,
            projections: Vec::new(),
        }
    }

    pub fn sync_target(mut self, v: Arc<dyn sync_target::SyncTarget>) -> Self {
        self.sync_target = Some(v);
        self
    }

    pub fn crypto(mut self, v: Arc<dyn crypto_provider::CryptoProvider>) -> Self {
        self.crypto = Some(v);
        self
    }

    pub fn ai(mut self, v: Arc<dyn ai_provider::AIProvider>) -> Self {
        self.ai = Some(v);
        self
    }

    pub fn kv_store(mut self, v: Arc<dyn kv_store::KVStore>) -> Self {
        self.kv_store = Some(v);
        self
    }

    pub fn search(mut self, v: Arc<dyn search_backend::SearchBackend>) -> Self {
        self.search = Some(v);
        self
    }

    pub fn ocr(mut self, v: Arc<dyn ocr_provider::OcrProvider>) -> Self {
        self.ocr = Some(v);
        self
    }

    pub fn plugin(mut self, v: Arc<dyn plugin_runtime::PluginRuntime>) -> Self {
        self.plugin = Some(v);
        self
    }

    pub fn event_bus_store(
        mut self,
        v: Arc<dyn crate::event_bus::layered::EventQueueStore>,
    ) -> Self {
        self.event_bus_store = Some(v);
        self
    }

    /// 注册读模型投影（V20 §4.5）。注册顺序即启动追赶顺序。
    pub fn projection(mut self, p: Arc<dyn crate::event_bus::projection::Projection>) -> Self {
        self.projections.push(p);
        self
    }

    /// 构建 `AppCore`。
    ///
    /// # Panics
    ///
    /// 任一必需 Trait 未注入时 panic（启动期校验，早失败）。
    pub fn build(mut self) -> AppCore {
        let event_bus = Arc::new(LayeredEventBus::new(self.event_bus_store));
        let projections = std::mem::take(&mut self.projections);
        let review_queue = Arc::new(crate::l3_domain::fsrs::ReviewQueue::new());

        AppCore {
            sync_target: self.sync_target.expect("SyncTarget must be provided"),
            crypto: self.crypto.expect("CryptoProvider must be provided"),
            ai: self.ai.expect("AIProvider must be provided"),
            kv_store: self.kv_store.expect("KVStore must be provided"),
            search: self.search.expect("SearchBackend must be provided"),
            ocr: self.ocr.expect("OcrProvider must be provided"),
            plugin: self.plugin.expect("PluginRuntime must be provided"),
            event_bus,
            projections,
            review_queue,
        }
    }
}

impl Default for AppCoreBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl AppCore {
    /// 启动时恢复流程（V19 ARCH-003）：
    /// 1. 重放未消费的 Medium 通道事件
    /// 2. 日志记录启动信息
    /// 投影增量追赶（V20 §4.5）— 启动时把全部注册投影从各自水位线
    /// 追赶到最新。幂等；verify 失败的投影自动全量重建（自愈）。
    ///
    /// async：由平台层在运行时（tokio block_on / spawn）调用。
    pub async fn catch_up_projections(&self) -> Result<(), Error> {
        for p in &self.projections {
            let n = self.event_bus.catch_up(p.as_ref()).await?;
            if n > 0 {
                info!(projection = p.name(), applied = n, "projection catch_up");
            }
        }
        Ok(())
    }

    /// 已注册投影列表（诊断 / 测试用）。
    pub fn projections(&self) -> &[Arc<dyn crate::event_bus::projection::Projection>] {
        &self.projections
    }

    /// 任务投影统计（TodayView 头部 — V20 §5.4.2）。
    /// 未注册任务投影时返回 (0, 0)。
    pub fn task_projection_stats(&self) -> (usize, usize) {
        for p in &self.projections {
            if let Some(tp) = p
                .as_any()
                .and_then(|a| a.downcast_ref::<crate::l2_engines::task_projection::TaskProjection>())
            {
                return tp.stats();
            }
        }
        (0, 0)
    }

    /// 双链反向查询（target 的入链 — 反链面板）。
    pub fn bidi_link_incoming(&self, target_note_id: &str) -> Vec<String> {
        for p in &self.projections {
            if let Some(bp) = p
                .as_any()
                .and_then(|a| a.downcast_ref::<crate::l2_engines::bidi_link_projection::BidiLinkProjection>())
            {
                return bp.incoming(target_note_id);
            }
        }
        Vec::new()
    }

    /// 双链正向查询（source 的出链 — 知识图谱）。
    pub fn bidi_link_outgoing(&self, source_note_id: &str) -> Vec<String> {
        for p in &self.projections {
            if let Some(bp) = p
                .as_any()
                .and_then(|a| a.downcast_ref::<crate::l2_engines::bidi_link_projection::BidiLinkProjection>())
            {
                return bp.outgoing(source_note_id);
            }
        }
        Vec::new()
    }

    /// 任务投影今日到期数（含逾期）。
    pub fn task_projection_due_today(&self) -> usize {
        for p in &self.projections {
            if let Some(tp) = p
                .as_any()
                .and_then(|a| a.downcast_ref::<crate::l2_engines::task_projection::TaskProjection>())
            {
                let now_ms = chrono::Utc::now().timestamp_millis();
                return tp.today(now_ms).len();
            }
        }
        0
    }

    pub fn startup(&self) -> Result<(), Error> {
        // V20 Phase 1: 先恢复全局 seq（防重启后新事件 seq 撞历史）
        self.event_bus.restore_seq()?;
        // ARCH-003：重放未消费事件
        match self.event_bus.replay_unconsumed() {
            Ok(events) => {
                if !events.is_empty() {
                    info!(count = events.len(), "replaying unconsumed events");
                    // 在实际实现中，将事件重新入队到各通道
                    for env in &events {
                        info!(
                            seq = env.seq,
                            event_type = env.event.event_type(),
                            "replayed"
                        );
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, "event replay failed; continuing with fresh state");
            }
        }

        // 记录版本信息（由构建方在注入前设置，这里仅日志）
        info!(
            crypto_version = self.crypto.algorithm_version(),
            "AppCore startup complete"
        );

        Ok(())
    }

    /// 启动 EventBus 消费者循环（Low 通道，含背压延迟）。
    ///
    /// 应在 tokio 运行时中 spawn；生产环境中由平台层调用。
    pub async fn run_low_consumer_loop(&self) {
        let mut rx = match self.event_bus.take_low_receiver() {
            Some(r) => r,
            None => {
                error!("low receiver already taken");
                return;
            }
        };

        info!("low channel consumer started");
        loop {
            // 背压检测
            if let Some(delay) = self.event_bus.low_channel_backpressure() {
                warn!(
                    delay_ms = delay.as_millis(),
                    "medium backlog detected; delaying low channel"
                );
                tokio::time::sleep(delay).await;
            }

            match rx.recv().await {
                Some(env) => {
                    info!(
                        seq = env.seq,
                        event_type = env.event.event_type(),
                        "low channel processing"
                    );
                    // 实际处理由 SearchEngine / VersionControl 等消费者实现
                    // 此处为事件总线框架层，仅做日志与背压
                }
                None => {
                    info!("low channel closed; exiting consumer loop");
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 注：完整 AppCore 测试需 mock 各 Trait 实现。
    // V19 §34.2 推荐使用 mockall 自动生成 mock，对应 test 在独立 test crate 中。

    #[test]
    fn builder_requires_all_traits() {
        // 缺失注入时应 panic（早失败策略）
        let result = std::panic::catch_unwind(|| {
            let _ = AppCoreBuilder::new().build();
        });
        assert!(result.is_err(), "builder should panic without trait impls");
    }
}
