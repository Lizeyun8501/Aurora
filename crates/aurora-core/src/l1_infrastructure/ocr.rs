//! OCR 引擎 (PaddleOCR / Tesseract)
//!
//! 提供图片文字识别 (OCR) 能力，用于从图片中提取文本内容。
//! 底层可选 PaddleOCR 或 Tesseract 作为识别引擎。

/// OCR 引擎句柄占位类型。
///
/// 实际实现将在后续任务中封装具体的 OCR 后端调用能力。
pub struct OcrEngine;
