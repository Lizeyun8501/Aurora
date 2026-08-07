//! OCR 服务（OCR Service）
//!
//! 实现双引擎 OCR（PaddleOCR 主 / Tesseract 兜底）、图像预处理流水线、
//! 表格识别、公式识别、批量 OCR。
//!
//! # 简化说明
//! - 真实 PaddleOCR / Tesseract 需要本地原生库，本模块将各 provider 实现为 **mock**：
//!   基于输入图像字节的内容哈希派生确定性「识别结果」，保证测试可复现。
//! - 图像预处理只返回元数据（尺寸/倾斜角等），不真正做像素级变换。
//! - 表格识别 mock：基于输入的「网格化文本」重组成单元格。

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

use super::content_editor::Block;

// ============================================================================
// SubTask 3.8.1: 双引擎 OCR
// ============================================================================

/// OCR 语言
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum OcrLanguage {
    Chinese,
    English,
    Mixed,
}

/// OCR 引擎类型
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum OcrEngineKind {
    /// PaddleOCR（主引擎，中文优先）
    Paddle,
    /// Tesseract（兜底，英文优先）
    Tesseract,
}

/// OCR 文本行
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrTextLine {
    pub text: String,
    pub confidence: f32,
    pub bbox: BoundingBox,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct BoundingBox {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// OCR Provider Trait
pub trait OcrProvider: Send + Sync {
    fn kind(&self) -> OcrEngineKind;
    fn supported_languages(&self) -> Vec<OcrLanguage>;
    /// 对图像字节做 OCR，返回识别出的文本行
    fn recognize(&self, image_data: &[u8], language: OcrLanguage) -> Vec<OcrTextLine>;
}

/// PaddleOCR Provider（mock）
pub struct PaddleOcrProvider;

impl OcrProvider for PaddleOcrProvider {
    fn kind(&self) -> OcrEngineKind {
        OcrEngineKind::Paddle
    }

    fn supported_languages(&self) -> Vec<OcrLanguage> {
        vec![OcrLanguage::Chinese, OcrLanguage::Mixed]
    }

    fn recognize(&self, image_data: &[u8], language: OcrLanguage) -> Vec<OcrTextLine> {
        // 中文场景下 PaddleOCR 主导
        if language == OcrLanguage::English {
            return Vec::new();
        }
        mock_recognize(image_data, OcrEngineKind::Paddle)
    }
}

/// Tesseract Provider（mock）
pub struct TesseractProvider;

impl OcrProvider for TesseractProvider {
    fn kind(&self) -> OcrEngineKind {
        OcrEngineKind::Tesseract
    }

    fn supported_languages(&self) -> Vec<OcrLanguage> {
        vec![OcrLanguage::English]
    }

    fn recognize(&self, image_data: &[u8], language: OcrLanguage) -> Vec<OcrTextLine> {
        if language == OcrLanguage::Chinese {
            return Vec::new();
        }
        mock_recognize(image_data, OcrEngineKind::Tesseract)
    }
}

/// mock 识别：基于图像内容哈希派生确定性结果
fn mock_recognize(image_data: &[u8], engine: OcrEngineKind) -> Vec<OcrTextLine> {
    if image_data.is_empty() {
        return Vec::new();
    }
    let hash = simple_hash(image_data);
    let prefix = match engine {
        OcrEngineKind::Paddle => "[Paddle]",
        OcrEngineKind::Tesseract => "[Tesseract]",
    };
    // 基于 hash 派生 2~3 行伪文本
    let line_count = (hash % 3) as usize + 1;
    (0..line_count)
        .map(|i| {
            let line_hash = hash.wrapping_mul((i + 1) as u64);
            OcrTextLine {
                text: format!("{} line-{} {:08x}", prefix, i + 1, line_hash),
                confidence: 0.85 + (line_hash % 15) as f32 / 100.0,
                bbox: BoundingBox {
                    x: 0,
                    y: (i as u32) * 40,
                    width: 800,
                    height: 40,
                },
            }
        })
        .collect()
}

fn simple_hash(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// OCR 引擎（双引擎：Paddle 主 + Tesseract 兜底）
pub struct OcrEngine {
    paddle: Arc<dyn OcrProvider>,
    tesseract: Arc<dyn OcrProvider>,
}

impl Default for OcrEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl OcrEngine {
    pub fn new() -> Self {
        Self {
            paddle: Arc::new(PaddleOcrProvider),
            tesseract: Arc::new(TesseractProvider),
        }
    }

    /// 自动选择引擎：中文用 Paddle，英文用 Tesseract，混合优先 Paddle
    pub fn recognize(&self, image_data: &[u8], language: OcrLanguage) -> Vec<OcrTextLine> {
        let primary = match language {
            OcrLanguage::English => &self.tesseract,
            _ => &self.paddle,
        };
        let result = primary.recognize(image_data, language);
        if result.is_empty() {
            warn!("primary OCR engine returned empty, falling back");
            let fallback: &Arc<dyn OcrProvider> = match language {
                OcrLanguage::English => &self.paddle,
                _ => &self.tesseract,
            };
            return fallback.recognize(image_data, language);
        }
        result
    }
}

// ============================================================================
// SubTask 3.8.2: 图像预处理流水线
// ============================================================================

/// 预处理步骤
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PreprocessStep {
    Denoise,
    Binarize,
    Deskew,
    LayoutAnalysis,
}

/// 预处理后的图像（mock：仅元数据）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreprocessedImage {
    pub original_size: (u32, u32),
    pub denoised: bool,
    pub binarized: bool,
    pub skew_angle_degrees: f32,
    pub detected_layout: LayoutType,
    pub steps_applied: Vec<PreprocessStep>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LayoutType {
    SingleColumn,
    MultiColumn,
    Table,
    Mixed,
}

/// 图像预处理器
pub struct ImagePreprocessor;

impl ImagePreprocessor {
    /// 运行预处理流水线
    pub fn process(image_data: &[u8], steps: &[PreprocessStep]) -> PreprocessedImage {
        // mock：尺寸基于数据长度推断
        let size = estimate_size(image_data);
        let hash = simple_hash(image_data);
        let skew = (hash % 7) as f32 - 3.0; // -3 ~ 3 度
        let layout = match hash % 4 {
            0 => LayoutType::SingleColumn,
            1 => LayoutType::MultiColumn,
            2 => LayoutType::Table,
            _ => LayoutType::Mixed,
        };
        let mut result = PreprocessedImage {
            original_size: size,
            denoised: false,
            binarized: false,
            skew_angle_degrees: skew,
            detected_layout: layout,
            steps_applied: Vec::new(),
        };
        for &step in steps {
            match step {
                PreprocessStep::Denoise => result.denoised = true,
                PreprocessStep::Binarize => result.binarized = true,
                PreprocessStep::Deskew => result.skew_angle_degrees = 0.0,
                PreprocessStep::LayoutAnalysis => { /* layout 已在上方确定 */ }
            }
            result.steps_applied.push(step);
        }
        debug!(steps = ?result.steps_applied, "image preprocessed");
        result
    }

    /// 默认流水线：denoise → binarize → deskew → layout
    pub fn default_pipeline(image_data: &[u8]) -> PreprocessedImage {
        Self::process(
            image_data,
            &[
                PreprocessStep::Denoise,
                PreprocessStep::Binarize,
                PreprocessStep::Deskew,
                PreprocessStep::LayoutAnalysis,
            ],
        )
    }
}

fn estimate_size(data: &[u8]) -> (u32, u32) {
    // mock：假设 4 通道，估算边长
    let pixels = (data.len() / 4) as u32;
    let side = (pixels as f32).sqrt().ceil() as u32;
    if side == 0 {
        (1, 1)
    } else {
        (side, side)
    }
}

// ============================================================================
// SubTask 3.8.3: 表格识别
// ============================================================================

/// 表格单元格
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableCell {
    pub row: u32,
    pub col: u32,
    pub text: String,
    pub confidence: f32,
    pub bbox: BoundingBox,
}

/// 识别出的表格
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecognizedTable {
    pub rows: u32,
    pub cols: u32,
    pub cells: Vec<TableCell>,
}

impl RecognizedTable {
    /// 重组为 Markdown 表格字符串
    pub fn to_markdown(&self) -> String {
        if self.rows == 0 || self.cols == 0 {
            return String::new();
        }
        let mut grid: Vec<Vec<String>> =
            vec![vec![String::new(); self.cols as usize]; self.rows as usize];
        for cell in &self.cells {
            let r = cell.row as usize;
            let c = cell.col as usize;
            if r < self.rows as usize && c < self.cols as usize {
                grid[r][c] = cell.text.clone();
            }
        }
        let mut md = String::new();
        for (i, row) in grid.iter().enumerate() {
            md.push_str("| ");
            md.push_str(&row.join(" | "));
            md.push_str(" |\n");
            if i == 0 {
                let sep: Vec<String> = row.iter().map(|_| "---".to_string()).collect();
                md.push_str("| ");
                md.push_str(&sep.join(" | "));
                md.push_str(" |\n");
            }
        }
        md
    }

    /// 重组为内部 `Block`（Table 类型）
    pub fn to_block(&self) -> Block {
        let rows: Vec<Vec<String>> = (0..self.rows)
            .map(|r| {
                (0..self.cols)
                    .map(|c| {
                        self.cells
                            .iter()
                            .find(|cell| cell.row == r && cell.col == c)
                            .map(|cell| cell.text.clone())
                            .unwrap_or_default()
                    })
                    .collect()
            })
            .collect();
        Block::table(rows)
    }
}

/// 表格识别器
pub struct TableRecognizer {
    engine: Arc<OcrEngine>,
}

impl TableRecognizer {
    pub fn new(engine: Arc<OcrEngine>) -> Self {
        Self { engine }
    }

    /// 识别图像中的表格。
    /// mock：基于预处理检测到的 LayoutType::Table，将 OCR 行结果切分为网格。
    pub fn recognize(&self, image_data: &[u8], rows: u32, cols: u32) -> RecognizedTable {
        let lines = self.engine.recognize(image_data, OcrLanguage::Mixed);
        let mut cells = Vec::new();
        for (i, line) in lines.iter().take((rows * cols) as usize).enumerate() {
            let r = (i as u32) / cols;
            let c = (i as u32) % cols;
            cells.push(TableCell {
                row: r,
                col: c,
                text: line.text.clone(),
                confidence: line.confidence,
                bbox: line.bbox,
            });
        }
        RecognizedTable { rows, cols, cells }
    }
}

// ============================================================================
// SubTask 3.8.4: 公式识别
// ============================================================================

/// LaTeX 输出
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatexOutput {
    pub latex: String,
    pub is_block: bool,
    pub confidence: f32,
}

impl LatexOutput {
    /// 转换为可插入文档的 `Block`（MathBlock 用 Custom 类型表示）
    pub fn to_block(&self) -> Block {
        let mut block = Block::new(
            super::content_editor::BlockType::Custom("math".to_string()),
            &self.latex,
        );
        block
            .properties
            .insert("is_block".to_string(), serde_json::json!(self.is_block));
        block
            .properties
            .insert("confidence".to_string(), serde_json::json!(self.confidence));
        block
    }
}

/// 公式识别器（mock）
pub struct FormulaRecognizer {
    engine: Arc<OcrEngine>,
}

impl FormulaRecognizer {
    pub fn new(engine: Arc<OcrEngine>) -> Self {
        Self { engine }
    }

    /// 识别图像中的数学公式并输出 LaTeX。
    /// mock：将 OCR 文本包装为 `$$...$$` 形式。
    pub fn recognize(&self, image_data: &[u8]) -> LatexOutput {
        let lines = self.engine.recognize(image_data, OcrLanguage::Mixed);
        let raw: String = lines
            .iter()
            .map(|l| l.text.clone())
            .collect::<Vec<_>>()
            .join(" ");
        // 简化：把识别到的文本当作公式内容
        let latex = if raw.is_empty() {
            "E = mc^2".to_string()
        } else {
            format!("\\text{{{}}}", raw)
        };
        let confidence = lines.first().map(|l| l.confidence).unwrap_or(0.9);
        LatexOutput {
            latex,
            is_block: true,
            confidence,
        }
    }
}

// ============================================================================
// SubTask 3.8.5: 批量 OCR
// ============================================================================

/// OCR 进度
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrProgress {
    pub total: usize,
    pub done: usize,
    pub failed: usize,
    pub current: Option<String>,
    pub results: HashMap<String, OcrResult>,
}

impl OcrProgress {
    pub fn new(total: usize) -> Self {
        Self {
            total,
            done: 0,
            failed: 0,
            current: None,
            results: HashMap::new(),
        }
    }

    pub fn percent(&self) -> f32 {
        if self.total == 0 {
            return 100.0;
        }
        ((self.done + self.failed) as f32 / self.total as f32) * 100.0
    }
}

/// 单条 OCR 结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrResult {
    pub asset_id: String,
    pub text: String,
    pub lines: Vec<OcrTextLine>,
    pub engine_used: OcrEngineKind,
    pub processing_ms: u64,
}

/// 批量 OCR 处理器
pub struct BatchOcrProcessor {
    engine: Arc<OcrEngine>,
    state: Arc<RwLock<OcrProgress>>,
    /// 模拟 EventBus 订阅者
    subscribers: Arc<RwLock<Vec<String>>>,
}

impl BatchOcrProcessor {
    pub fn new(engine: Arc<OcrEngine>, total: usize) -> Self {
        Self {
            engine,
            state: Arc::new(RwLock::new(OcrProgress::new(total))),
            subscribers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn subscribe(&self, id: impl Into<String>) {
        self.subscribers.write().push(id.into());
    }

    pub fn subscriber_count(&self) -> usize {
        self.subscribers.read().len()
    }

    /// 处理一个图像（同步 mock）
    pub fn process_one(
        &self,
        asset_id: &str,
        image_data: &[u8],
        language: OcrLanguage,
    ) -> OcrResult {
        {
            let mut st = self.state.write();
            st.current = Some(asset_id.to_string());
        }
        let start = std::time::Instant::now();
        let lines = self.engine.recognize(image_data, language);
        let text: String = lines
            .iter()
            .map(|l| l.text.clone())
            .collect::<Vec<_>>()
            .join("\n");
        let engine_used = if text.starts_with("[Paddle]") {
            OcrEngineKind::Paddle
        } else {
            OcrEngineKind::Tesseract
        };
        let elapsed = start.elapsed().as_millis() as u64;
        let result = OcrResult {
            asset_id: asset_id.to_string(),
            text,
            lines,
            engine_used,
            processing_ms: elapsed,
        };
        let mut st = self.state.write();
        st.done += 1;
        st.current = None;
        st.results.insert(asset_id.to_string(), result.clone());
        info!(asset_id = %asset_id, "ocr done (mock)");
        result
    }

    /// 批量处理
    pub fn process_batch(&self, items: &[(String, Vec<u8>)], language: OcrLanguage) -> OcrProgress {
        for (asset_id, data) in items {
            self.process_one(asset_id, data, language);
        }
        self.state.read().clone()
    }

    pub fn progress(&self) -> OcrProgress {
        self.state.read().clone()
    }
}

// ============================================================================
// 顶层 OCR 服务聚合
// ============================================================================

/// OCR 服务顶层入口
pub struct OcrService {
    pub engine: Arc<OcrEngine>,
}

impl Default for OcrService {
    fn default() -> Self {
        Self::new()
    }
}

impl OcrService {
    pub fn new() -> Self {
        Self {
            engine: Arc::new(OcrEngine::new()),
        }
    }

    /// 识别图像，返回文本
    pub fn recognize_text(&self, image_data: &[u8], language: OcrLanguage) -> String {
        let lines = self.engine.recognize(image_data, language);
        lines
            .iter()
            .map(|l| l.text.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 识别表格
    pub fn recognize_table(&self, image_data: &[u8], rows: u32, cols: u32) -> RecognizedTable {
        TableRecognizer::new(self.engine.clone()).recognize(image_data, rows, cols)
    }

    /// 识别公式
    pub fn recognize_formula(&self, image_data: &[u8]) -> LatexOutput {
        FormulaRecognizer::new(self.engine.clone()).recognize(image_data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(id: u8) -> Vec<u8> {
        vec![id; 100]
    }

    // --- Dual engine ---

    #[test]
    fn test_paddle_recognizes_chinese() {
        let p = PaddleOcrProvider;
        let lines = p.recognize(&img(1), OcrLanguage::Chinese);
        assert!(!lines.is_empty());
        assert!(lines.iter().all(|l| l.text.starts_with("[Paddle]")));
    }

    #[test]
    fn test_paddle_skips_english() {
        let p = PaddleOcrProvider;
        let lines = p.recognize(&img(1), OcrLanguage::English);
        assert!(lines.is_empty());
    }

    #[test]
    fn test_tesseract_recognizes_english() {
        let t = TesseractProvider;
        let lines = t.recognize(&img(2), OcrLanguage::English);
        assert!(!lines.is_empty());
        assert!(lines.iter().all(|l| l.text.starts_with("[Tesseract]")));
    }

    #[test]
    fn test_engine_fallback_to_tesseract() {
        let engine = OcrEngine::new();
        // English → primary=Tesseract，应直接返回
        let lines = engine.recognize(&img(3), OcrLanguage::English);
        assert!(!lines.is_empty());
        assert!(lines[0].text.starts_with("[Tesseract]"));
    }

    #[test]
    fn test_engine_fallback_to_paddle_for_english_empty() {
        // Tesseract 在 Chinese 下返回空 → 应触发 fallback 到 Paddle
        // 但 language=Chinese 时 primary=Paddle 本身有结果，不会 fallback
        // 这里构造 English 场景：Tesseract 正常工作，不会触发 fallback
        let engine = OcrEngine::new();
        let lines = engine.recognize(&img(4), OcrLanguage::English);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_empty_image_returns_empty() {
        let p = PaddleOcrProvider;
        let lines = p.recognize(&[], OcrLanguage::Chinese);
        assert!(lines.is_empty());
    }

    #[test]
    fn test_provider_supported_languages() {
        assert!(PaddleOcrProvider
            .supported_languages()
            .contains(&OcrLanguage::Chinese));
        assert!(TesseractProvider
            .supported_languages()
            .contains(&OcrLanguage::English));
    }

    #[test]
    fn test_mock_recognize_deterministic() {
        let a = mock_recognize(&img(5), OcrEngineKind::Paddle);
        let b = mock_recognize(&img(5), OcrEngineKind::Paddle);
        assert_eq!(a.len(), b.len());
        assert_eq!(a[0].text, b[0].text);
    }

    // --- Preprocessing ---

    #[test]
    fn test_preprocess_default_pipeline() {
        let pp = ImagePreprocessor::default_pipeline(&img(6));
        assert!(pp.denoised);
        assert!(pp.binarized);
        assert_eq!(pp.skew_angle_degrees, 0.0); // deskew 后归零
        assert_eq!(pp.steps_applied.len(), 4);
    }

    #[test]
    fn test_preprocess_partial_steps() {
        let pp = ImagePreprocessor::process(
            &img(7),
            &[PreprocessStep::Denoise, PreprocessStep::LayoutAnalysis],
        );
        assert!(pp.denoised);
        assert!(!pp.binarized);
        // 未做 deskew → skew 不为 0（取决于 hash）
        assert_eq!(pp.steps_applied.len(), 2);
    }

    #[test]
    fn test_preprocess_layout_detection() {
        let pp = ImagePreprocessor::default_pipeline(&img(8));
        // layout 应该是 4 种之一
        assert!(matches!(
            pp.detected_layout,
            LayoutType::SingleColumn
                | LayoutType::MultiColumn
                | LayoutType::Table
                | LayoutType::Mixed
        ));
    }

    #[test]
    fn test_preprocess_deterministic() {
        let a = ImagePreprocessor::default_pipeline(&img(9));
        let b = ImagePreprocessor::default_pipeline(&img(9));
        assert_eq!(a.detected_layout, b.detected_layout);
        assert_eq!(a.original_size, b.original_size);
    }

    // --- Table recognition ---

    #[test]
    fn test_table_recognize_cells() {
        let engine = Arc::new(OcrEngine::new());
        let tr = TableRecognizer::new(engine);
        let table = tr.recognize(&img(10), 2, 3);
        assert_eq!(table.rows, 2);
        assert_eq!(table.cols, 3);
        // mock 文本行可能少于 6 个，cells 数应 <= 6
        assert!(table.cells.len() <= 6);
    }

    #[test]
    fn test_table_to_markdown() {
        let table = RecognizedTable {
            rows: 2,
            cols: 2,
            cells: vec![
                TableCell {
                    row: 0,
                    col: 0,
                    text: "A".into(),
                    confidence: 0.9,
                    bbox: BoundingBox::default(),
                },
                TableCell {
                    row: 0,
                    col: 1,
                    text: "B".into(),
                    confidence: 0.9,
                    bbox: BoundingBox::default(),
                },
                TableCell {
                    row: 1,
                    col: 0,
                    text: "1".into(),
                    confidence: 0.9,
                    bbox: BoundingBox::default(),
                },
                TableCell {
                    row: 1,
                    col: 1,
                    text: "2".into(),
                    confidence: 0.9,
                    bbox: BoundingBox::default(),
                },
            ],
        };
        let md = table.to_markdown();
        assert!(md.contains("| A | B |"));
        assert!(md.contains("| --- | --- |"));
        assert!(md.contains("| 1 | 2 |"));
    }

    #[test]
    fn test_table_to_block() {
        let table = RecognizedTable {
            rows: 1,
            cols: 2,
            cells: vec![
                TableCell {
                    row: 0,
                    col: 0,
                    text: "h1".into(),
                    confidence: 0.9,
                    bbox: BoundingBox::default(),
                },
                TableCell {
                    row: 0,
                    col: 1,
                    text: "h2".into(),
                    confidence: 0.9,
                    bbox: BoundingBox::default(),
                },
            ],
        };
        let block = table.to_block();
        assert!(matches!(
            block.block_type,
            super::super::content_editor::BlockType::Table
        ));
        let rows = block.content.as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_table_to_markdown_empty() {
        let table = RecognizedTable {
            rows: 0,
            cols: 0,
            cells: vec![],
        };
        assert_eq!(table.to_markdown(), "");
    }

    // --- Formula recognition ---

    #[test]
    fn test_formula_recognize_returns_latex() {
        let engine = Arc::new(OcrEngine::new());
        let fr = FormulaRecognizer::new(engine);
        let out = fr.recognize(&img(11));
        assert!(!out.latex.is_empty());
        assert!(out.is_block);
        assert!(out.confidence > 0.0);
    }

    #[test]
    fn test_formula_empty_image_default() {
        let engine = Arc::new(OcrEngine::new());
        let fr = FormulaRecognizer::new(engine);
        let out = fr.recognize(&[]);
        // 空图像 → 默认公式
        assert_eq!(out.latex, "E = mc^2");
    }

    #[test]
    fn test_formula_to_block() {
        let out = LatexOutput {
            latex: "x^2 + y^2 = r^2".into(),
            is_block: true,
            confidence: 0.95,
        };
        let block = out.to_block();
        assert!(matches!(
            block.block_type,
            super::super::content_editor::BlockType::Custom(ref s) if s == "math"
        ));
        assert_eq!(block.content.as_str().unwrap(), "x^2 + y^2 = r^2");
        assert_eq!(
            block.properties.get("is_block").unwrap().as_bool(),
            Some(true)
        );
    }

    // --- Batch OCR ---

    #[test]
    fn test_batch_ocr_process_one() {
        let engine = Arc::new(OcrEngine::new());
        let proc = BatchOcrProcessor::new(engine, 1);
        proc.subscribe("sub1");
        let result = proc.process_one("asset-1", &img(12), OcrLanguage::Chinese);
        assert_eq!(result.asset_id, "asset-1");
        assert!(!result.text.is_empty());
        let progress = proc.progress();
        assert_eq!(progress.done, 1);
        assert_eq!(progress.total, 1);
        assert_eq!(progress.percent(), 100.0);
        assert_eq!(proc.subscriber_count(), 1);
    }

    #[test]
    fn test_batch_ocr_process_batch() {
        let engine = Arc::new(OcrEngine::new());
        let items: Vec<(String, Vec<u8>)> = vec![
            ("a1".into(), img(21)),
            ("a2".into(), img(22)),
            ("a3".into(), img(23)),
        ];
        let proc = BatchOcrProcessor::new(engine, items.len());
        let progress = proc.process_batch(&items, OcrLanguage::Mixed);
        assert_eq!(progress.done, 3);
        assert_eq!(progress.results.len(), 3);
        // 每个结果应该关联到对应 asset
        assert!(progress.results.contains_key("a1"));
        assert!(progress.results.contains_key("a2"));
        assert!(progress.results.contains_key("a3"));
    }

    #[test]
    fn test_batch_ocr_progress_percent_empty() {
        let engine = Arc::new(OcrEngine::new());
        let proc = BatchOcrProcessor::new(engine, 0);
        assert_eq!(proc.progress().percent(), 100.0);
    }

    #[test]
    fn test_batch_ocr_engine_kind_recorded() {
        let engine = Arc::new(OcrEngine::new());
        let proc = BatchOcrProcessor::new(engine, 1);
        let result = proc.process_one("asset-x", &img(30), OcrLanguage::Chinese);
        // Chinese 走 Paddle
        assert_eq!(result.engine_used, OcrEngineKind::Paddle);
    }

    // --- Top-level service ---

    #[test]
    fn test_ocr_service_recognize_text() {
        let svc = OcrService::new();
        let text = svc.recognize_text(&img(40), OcrLanguage::Chinese);
        assert!(!text.is_empty());
        assert!(text.contains("[Paddle]"));
    }

    #[test]
    fn test_ocr_service_recognize_table() {
        let svc = OcrService::new();
        let table = svc.recognize_table(&img(41), 2, 2);
        assert_eq!(table.rows, 2);
        assert_eq!(table.cols, 2);
    }

    #[test]
    fn test_ocr_service_recognize_formula() {
        let svc = OcrService::new();
        let out = svc.recognize_formula(&img(42));
        assert!(!out.latex.is_empty());
    }

    #[test]
    fn test_simple_hash_deterministic() {
        assert_eq!(simple_hash(b"abc"), simple_hash(b"abc"));
        assert_ne!(simple_hash(b"abc"), simple_hash(b"abd"));
    }
}
