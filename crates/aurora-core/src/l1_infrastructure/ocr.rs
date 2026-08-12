//! OCR 引擎 (PaddleOCR / Tesseract)
//!
//! 提供图片文字识别 (OCR) 能力，用于从图片中提取文本内容。
//! 底层可选 PaddleOCR 或 Tesseract 作为识别引擎。

use async_trait::async_trait;
use crate::traits::ocr_provider::{OcrProvider, OcrResult};

/// PaddleOCR 引擎实现。
///
/// 当前为接口占位，真实推理需接入 PaddleOCR C++ 库或 ONNX 运行时。
pub struct PaddleOcrEngine;

impl PaddleOcrEngine {
    /// 创建新的 PaddleOCR 引擎实例。
    pub fn new() -> Self {
        Self
    }
}

impl Default for PaddleOcrEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl OcrProvider for PaddleOcrEngine {
    async fn recognize(&self, _image_bytes: &[u8]) -> Result<OcrResult, crate::Error> {
        tracing::warn!("PaddleOcrEngine::recognize is not yet implemented");
        Err(crate::Error::Internal(
            "PaddleOCR backend not linked".to_string(),
        ))
    }

    async fn recognize_batch(&self, _image_bytes_list: &[&[u8]]) -> Result<Vec<OcrResult>, crate::Error> {
        tracing::warn!("PaddleOcrEngine::recognize_batch is not yet implemented");
        Err(crate::Error::Internal(
            "PaddleOCR backend not linked".to_string(),
        ))
    }

    fn is_available(&self) -> bool {
        false
    }
}

/// Tesseract 引擎实现。
///
/// 当前为接口占位，真实推理需接入 tesseract 系统库或 Rust bindings。
pub struct TesseractEngine;

impl TesseractEngine {
    /// 创建新的 Tesseract 引擎实例。
    pub fn new() -> Self {
        Self
    }
}

impl Default for TesseractEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl OcrProvider for TesseractEngine {
    async fn recognize(&self, _image_bytes: &[u8]) -> Result<OcrResult, crate::Error> {
        tracing::warn!("TesseractEngine::recognize is not yet implemented");
        Err(crate::Error::Internal(
            "Tesseract backend not linked".to_string(),
        ))
    }

    async fn recognize_batch(&self, _image_bytes_list: &[&[u8]]) -> Result<Vec<OcrResult>, crate::Error> {
        tracing::warn!("TesseractEngine::recognize_batch is not yet implemented");
        Err(crate::Error::Internal(
            "Tesseract backend not linked".to_string(),
        ))
    }

    fn is_available(&self) -> bool {
        false
    }
}
