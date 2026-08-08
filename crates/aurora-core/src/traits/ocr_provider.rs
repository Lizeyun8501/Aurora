//! Trait: OCRProvider — 图片文字识别引擎的统一接口
//!
//! V19 §28 原始指定 `async_trait`，本批次 PR 推进异步化迁移。
//! 纯查询方法 `is_available` 保持同步签名。

use async_trait::async_trait;

/// OCR 识别结果
#[derive(Debug, Clone)]
pub struct OcrResult {
    pub text: String,
    pub confidence: f32,
}

/// OCR 引擎统一接口
#[async_trait]
pub trait OcrProvider: Send + Sync {
    /// 对单张图片进行文字识别
    async fn recognize(&self, image_bytes: &[u8]) -> Result<OcrResult, crate::Error>;

    /// 对多张图片进行批量文字识别
    async fn recognize_batch(
        &self,
        image_bytes_list: &[&[u8]],
    ) -> Result<Vec<OcrResult>, crate::Error>;

    /// 检查 OCR 引擎是否可用（纯查询，保持同步签名）
    fn is_available(&self) -> bool;
}
