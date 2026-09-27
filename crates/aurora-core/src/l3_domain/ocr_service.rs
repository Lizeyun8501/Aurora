//! OCR 服务（OCR Service）
//!
//! 实现双引擎 OCR（PaddleOCR 主 / Tesseract 兜底）、图像预处理流水线、
//! 表格识别、公式识别、批量 OCR。
//!
//! # 诚实化声明（DK-16，伪成功铁律）
//!
//! 真实 PaddleOCR / Tesseract 需要本地原生库，**尚未接入**。依「禁止伪成功」铁律，
//! 原占位路径（伪造识别文本与置信度）已全部移除：
//! 所有 `recognize` 入口现返回 `Err(OcrError::NotImplemented)`，调用方据此
//! 降级（绝不产生可进搜索索引的伪识别结果）。
//! 真实引擎接入挂载点：`OcrProvider::recognize` 的两个 provider 实现体
//! （接入时引入 feature-gate 与原生依赖，替换 Err 分支）。
//! - 图像预处理只返回启发式元数据（尺寸/倾斜角等），不真正做像素级变换。

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
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

/// OCR 错误（DK-16 诚实化：引擎未接入时明确报错，绝不返回伪造结果）。
///
/// 语义对应错误码字典 `ErrorCode::D07`（「此功能需要 OCR 引擎」，
/// FallbackAction::NotImplemented）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OcrError {
    /// 引擎未接入真实实现（mock 已按伪成功铁律移除）。
    /// 真实引擎接入挂载点见模块文档。
    NotImplemented { engine: OcrEngineKind },
    /// 图像中无可识别内容（真实引擎下才可能出现；保留供接入后使用）。
    NoRecognizableContent,
}

impl fmt::Display for OcrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OcrError::NotImplemented { engine } => write!(
                f,
                "OCR 引擎 {engine:?} 未接入真实实现（对应错误码 D07：此功能需要 OCR 引擎）"
            ),
            OcrError::NoRecognizableContent => {
                write!(f, "图像中无可识别内容")
            }
        }
    }
}

impl std::error::Error for OcrError {}

/// OCR Provider Trait
pub trait OcrProvider: Send + Sync {
    fn kind(&self) -> OcrEngineKind;
    fn supported_languages(&self) -> Vec<OcrLanguage>;
    /// 对图像字节做 OCR，返回识别出的文本行。
    /// DK-16 诚实化：引擎未接入时返回 `Err(OcrError::NotImplemented)`。
    fn recognize(
        &self,
        image_data: &[u8],
        language: OcrLanguage,
    ) -> Result<Vec<OcrTextLine>, OcrError>;
}

/// PaddleOCR Provider（真实引擎未接入——DK-16 诚实化，挂载点见模块文档）
pub struct PaddleOcrProvider;

impl OcrProvider for PaddleOcrProvider {
    fn kind(&self) -> OcrEngineKind {
        OcrEngineKind::Paddle
    }

    fn supported_languages(&self) -> Vec<OcrLanguage> {
        vec![OcrLanguage::Chinese, OcrLanguage::Mixed]
    }

    fn recognize(
        &self,
        _image_data: &[u8],
        _language: OcrLanguage,
    ) -> Result<Vec<OcrTextLine>, OcrError> {
        // 真实 PaddleOCR 接入挂载点：原生库绑定 + feature-gate（接入前禁止任何伪成功路径）
        Err(OcrError::NotImplemented {
            engine: OcrEngineKind::Paddle,
        })
    }
}

/// Tesseract Provider（真实引擎未接入——DK-16 诚实化，挂载点见模块文档）
pub struct TesseractProvider;

impl OcrProvider for TesseractProvider {
    fn kind(&self) -> OcrEngineKind {
        OcrEngineKind::Tesseract
    }

    fn supported_languages(&self) -> Vec<OcrLanguage> {
        vec![OcrLanguage::English]
    }

    fn recognize(
        &self,
        _image_data: &[u8],
        _language: OcrLanguage,
    ) -> Result<Vec<OcrTextLine>, OcrError> {
        // 真实 Tesseract 接入挂载点：原生库绑定 + feature-gate（接入前禁止任何伪成功路径）
        Err(OcrError::NotImplemented {
            engine: OcrEngineKind::Tesseract,
        })
    }
}

/// 启发式内容指纹（仅用于图像预处理元数据推断：尺寸/布局启发），不派生任何「识别结果」。
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

    /// 自动选择引擎：中文用 Paddle，英文用 Tesseract，混合优先 Paddle。
    /// primary 报 NotImplemented 时尝试兜底引擎；两者均未接入则传播错误。
    pub fn recognize(
        &self,
        image_data: &[u8],
        language: OcrLanguage,
    ) -> Result<Vec<OcrTextLine>, OcrError> {
        let primary = match language {
            OcrLanguage::English => &self.tesseract,
            _ => &self.paddle,
        };
        match primary.recognize(image_data, language) {
            Ok(lines) => Ok(lines),
            Err(primary_err) => {
                let fallback: &Arc<dyn OcrProvider> = match language {
                    OcrLanguage::English => &self.paddle,
                    _ => &self.tesseract,
                };
                match fallback.recognize(image_data, language) {
                    Ok(lines) => {
                        warn!("primary OCR engine unavailable, fallback succeeded");
                        Ok(lines)
                    }
                    Err(_) => {
                        // 两个引擎均未接入：如实传播 primary 的 NotImplemented（错误码 D07 语义）
                        Err(primary_err)
                    }
                }
            }
        }
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

/// 预处理后的图像（DK-16 诚实化：当前为启发式元数据推断，非真实像素级变换；真实变换接入后逐字段替换）
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
        // 启发式：尺寸基于数据长度推断（元数据，非识别结果）
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
    // 启发式：假设 4 通道，估算边长（元数据）
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
    /// 基于预处理检测到的 LayoutType::Table，将 OCR 行结果切分为网格。
    ///（网格重组本身是通用逻辑；行数据来源已诚实化为 Result 通路）
    pub fn recognize(
        &self,
        image_data: &[u8],
        rows: u32,
        cols: u32,
    ) -> Result<RecognizedTable, OcrError> {
        let lines = self.engine.recognize(image_data, OcrLanguage::Mixed)?;
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
        Ok(RecognizedTable { rows, cols, cells })
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
    pub fn recognize(&self, image_data: &[u8]) -> Result<LatexOutput, OcrError> {
        let lines = self.engine.recognize(image_data, OcrLanguage::Mixed)?;
        let raw: String = lines
            .iter()
            .map(|l| l.text.clone())
            .collect::<Vec<_>>()
            .join(" ");
        if raw.is_empty() {
            // 诚实化：无可识别内容必须显式报错（原硬编码默认公式属伪造结果）
            return Err(OcrError::NoRecognizableContent);
        }
        // 简化：把识别到的文本当作公式内容（真实 LaTeX 生成待引擎接入）
        let latex = format!("\\text{{{}}}", raw);
        let confidence = lines.first().map(|l| l.confidence).unwrap_or(0.9);
        Ok(LatexOutput {
            latex,
            is_block: true,
            confidence,
        })
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
    ) -> Result<OcrResult, OcrError> {
        {
            let mut st = self.state.write();
            st.current = Some(asset_id.to_string());
        }
        let start = std::time::Instant::now();
        // DK-16 诚实化：引擎未接入时计入 failed 并显式报错（failed 字段自此有真实语义）
        let lines = match self.engine.recognize(image_data, language) {
            Ok(lines) => lines,
            Err(e) => {
                let mut st = self.state.write();
                st.failed += 1;
                st.current = None;
                return Err(e);
            }
        };
        let text: String = lines
            .iter()
            .map(|l| l.text.clone())
            .collect::<Vec<_>>()
            .join("\n");
        // engine_used 按语言选择语义推导（与 OcrEngine::recognize 的 primary 选择一致）
        let engine_used = match language {
            OcrLanguage::English => OcrEngineKind::Tesseract,
            _ => OcrEngineKind::Paddle,
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
        info!(asset_id = %asset_id, "ocr item done");
        Ok(result)
    }

    /// 批量处理
    pub fn process_batch(&self, items: &[(String, Vec<u8>)], language: OcrLanguage) -> OcrProgress {
        for (asset_id, data) in items {
            // 失败项计入 failed（process_one 内部处理），结果不进 results——丢弃 Err 属预期降级语义
            let _ = self.process_one(asset_id, data, language);
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

    /// 识别图像，返回文本（引擎未接入时 Err(OcrError::NotImplemented)，对应错误码 D07）
    pub fn recognize_text(
        &self,
        image_data: &[u8],
        language: OcrLanguage,
    ) -> Result<String, OcrError> {
        let lines = self.engine.recognize(image_data, language)?;
        Ok(lines
            .iter()
            .map(|l| l.text.clone())
            .collect::<Vec<_>>()
            .join("\n"))
    }

    /// 识别表格
    pub fn recognize_table(
        &self,
        image_data: &[u8],
        rows: u32,
        cols: u32,
    ) -> Result<RecognizedTable, OcrError> {
        TableRecognizer::new(self.engine.clone()).recognize(image_data, rows, cols)
    }

    /// 识别公式
    pub fn recognize_formula(&self, image_data: &[u8]) -> Result<LatexOutput, OcrError> {
        FormulaRecognizer::new(self.engine.clone()).recognize(image_data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(id: u8) -> Vec<u8> {
        vec![id; 100]
    }

    // --- DK-16 诚实化：NotImplemented 断言（禁止任何 Ok 伪成功路径） ---

    #[test]
    fn test_paddle_not_implemented() {
        let p = PaddleOcrProvider;
        let r = p.recognize(&img(1), OcrLanguage::Chinese);
        assert!(
            matches!(
                r,
                Err(OcrError::NotImplemented {
                    engine: OcrEngineKind::Paddle
                })
            ),
            "Paddle 必须显式 NotImplemented，禁止 Ok 伪成功: {r:?}"
        );
    }

    #[test]
    fn test_tesseract_not_implemented() {
        let p = TesseractProvider;
        let r = p.recognize(&img(2), OcrLanguage::English);
        assert!(
            matches!(
                r,
                Err(OcrError::NotImplemented {
                    engine: OcrEngineKind::Tesseract
                })
            ),
            "Tesseract 必须显式 NotImplemented: {r:?}"
        );
    }

    #[test]
    fn test_engine_recognize_err_for_all_languages() {
        // 三种语言 + 空图：全部 Err（引擎 fallback 也不得伪造）
        let engine = OcrEngine::new();
        for lang in [
            OcrLanguage::Chinese,
            OcrLanguage::English,
            OcrLanguage::Mixed,
        ] {
            let r = engine.recognize(&img(3), lang);
            assert!(
                matches!(r, Err(OcrError::NotImplemented { .. })),
                "lang={lang:?} 必须 Err(NotImplemented): {r:?}"
            );
            let r_empty = engine.recognize(&[], lang);
            assert!(matches!(r_empty, Err(OcrError::NotImplemented { .. })));
        }
    }

    #[test]
    fn test_ocr_error_display_mentions_d07() {
        let e = OcrError::NotImplemented {
            engine: OcrEngineKind::Paddle,
        };
        let s = e.to_string();
        assert!(s.contains("D07"), "错误信息应引用错误码字典 D07: {s}");
        assert!(s.contains("未接入"));
    }

    #[test]
    fn test_service_recognize_text_err() {
        let svc = OcrService::new();
        let r = svc.recognize_text(&img(4), OcrLanguage::Mixed);
        assert!(matches!(r, Err(OcrError::NotImplemented { .. })));
    }

    #[test]
    fn test_service_recognize_table_err() {
        let svc = OcrService::new();
        let r = svc.recognize_table(&img(5), 2, 3);
        assert!(matches!(r, Err(OcrError::NotImplemented { .. })));
    }

    #[test]
    fn test_service_recognize_formula_err() {
        let svc = OcrService::new();
        let r = svc.recognize_formula(&img(6));
        assert!(matches!(r, Err(OcrError::NotImplemented { .. })));
    }

    // --- 批量：失败计入 failed（failed 字段自此有真实语义） ---

    #[test]
    fn test_batch_counts_failures() {
        let engine = Arc::new(OcrEngine::new());
        let batch = BatchOcrProcessor::new(engine, 2);
        let progress = batch.process_batch(
            &[("a".to_string(), img(7)), ("b".to_string(), img(8))],
            OcrLanguage::Mixed,
        );
        assert_eq!(progress.failed, 2, "未接入引擎的批量项应计入 failed");
        assert_eq!(progress.done, 0);
        assert!(progress.results.is_empty(), "失败项不得进 results");
    }

    #[test]
    fn test_batch_percent_accounts_failures() {
        let engine = Arc::new(OcrEngine::new());
        let batch = BatchOcrProcessor::new(engine, 1);
        let progress = batch.process_batch(&[("a".to_string(), img(9))], OcrLanguage::Mixed);
        assert!((progress.percent() - 100.0).abs() < f32::EPSILON);
    }

    // --- 预处理：元数据语义不受诚实化影响（无识别结果派生） ---

    #[test]
    fn test_preprocess_still_metadata_only() {
        let pre = ImagePreprocessor::default_pipeline(&img(10));
        assert!(pre.steps_applied.len() <= 4);
    }

    #[test]
    fn test_provider_supported_languages_unchanged() {
        assert!(PaddleOcrProvider
            .supported_languages()
            .contains(&OcrLanguage::Chinese));
        assert!(TesseractProvider
            .supported_languages()
            .contains(&OcrLanguage::English));
    }
}
