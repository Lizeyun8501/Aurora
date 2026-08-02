//! Trait: OCRProvider — 图片文字识别引擎的统一接口

/// OCR 识别结果
#[derive(Debug, Clone)]
pub struct OcrResult {
    pub text: String,
    pub confidence: f32,
}

/// OCR 引擎统一接口
pub trait OcrProvider: Send + Sync {
    /// 对单张图片进行文字识别
    fn recognize(&self, image_bytes: &[u8]) -> Result<OcrResult, crate::Error>;

    /// 对多张图片进行批量文字识别
    fn recognize_batch(&self, image_bytes_list: &[&[u8]]) -> Result<Vec<OcrResult>, crate::Error>;

    /// 检查 OCR 引擎是否可用
    fn is_available(&self) -> bool;
}
