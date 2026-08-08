//! 素材库系统（Asset Library System）
//!
//! 实现内容寻址存储、缩略图生成、EXIF 元数据解析、重复检测。
//!
//! # 简化说明
//! - 内容寻址使用 `sha3::Sha3_256`（Keccak-256，工作区已有依赖）作为内容哈希，
//!   语义上等价于"SHA-256 风格的内容寻址"。
//! - 缩略图生成不调用真实 `image` / `ffmpeg`，仅基于内容哈希派生一个确定性的
//!   缩略图标识（mock）。
//! - EXIF 解析不依赖 `kamadak-exif`，而是从一个简化的标签结构中提取
//!   datetime / GPS / device 信息。
//! - pHash 使用「下采样到 8x8 灰度 + 平均哈希（aHash）」实现，并计算汉明距离。

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::{debug, info};

// ============================================================================
// SubTask 3.5.1: 内容寻址存储
// ============================================================================

/// 素材唯一标识（内容寻址：基于 SHA3-256 哈希）
pub type AssetId = String;

/// 素材类型
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AssetType {
    Image,
    Video,
    Audio,
    Document,
    Other,
}

impl AssetType {
    pub fn from_mime(mime: &str) -> Self {
        if mime.starts_with("image/") {
            AssetType::Image
        } else if mime.starts_with("video/") {
            AssetType::Video
        } else if mime.starts_with("audio/") {
            AssetType::Audio
        } else if mime.starts_with("application/")
            || mime == "text/plain"
            || mime == "application/pdf"
        {
            AssetType::Document
        } else {
            AssetType::Other
        }
    }
}

/// 素材元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub id: AssetId,
    pub original_name: String,
    pub mime_type: String,
    pub asset_type: AssetType,
    pub size_bytes: u64,
    /// 内容哈希（SHA3-256 hex）
    pub content_hash: String,
    /// 存储路径（相对路径或 URI）
    pub storage_path: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub exif: Option<ExifMetadata>,
    pub thumbnail: Option<Thumbnail>,
    /// 感知哈希（用于重复检测）
    pub phash: Option<PerceptualHash>,
}

impl Asset {
    pub fn new(
        original_name: impl Into<String>,
        mime_type: impl Into<String>,
        data: &[u8],
    ) -> Self {
        let mime = mime_type.into();
        let hash = content_hash_hex(data);
        let now = chrono::Utc::now();
        Asset {
            // 内容寻址：相同内容 → 相同 ID（天然去重键）
            id: hash.clone(),
            original_name: original_name.into(),
            asset_type: AssetType::from_mime(&mime),
            mime_type: mime,
            size_bytes: data.len() as u64,
            content_hash: hash.clone(),
            storage_path: format!("assets/{}/{}", &hash[..2], hash),
            created_at: now,
            exif: None,
            thumbnail: None,
            phash: None,
        }
    }
}

/// 计算内容的 SHA3-256 十六进制摘要
pub fn content_hash_hex(data: &[u8]) -> String {
    let mut hasher = Sha3_256::new();
    hasher.update(data);
    let bytes = hasher.finalize();
    hex_encode(bytes.as_slice())
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// 素材存储（内容寻址 + 自然去重）
pub struct AssetStore {
    /// id → Asset
    assets: Arc<RwLock<HashMap<AssetId, Asset>>>,
    /// content_hash → 已存在的 AssetId（用于去重判断）
    hash_index: Arc<RwLock<HashMap<String, AssetId>>>,
    /// 模拟 SQLite assets 表：id → 行记录（这里用 JSON Value 表示）
    table: Arc<RwLock<Vec<serde_json::Value>>>,
}

impl Default for AssetStore {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetStore {
    pub fn new() -> Self {
        Self {
            assets: Arc::new(RwLock::new(HashMap::new())),
            hash_index: Arc::new(RwLock::new(HashMap::new())),
            table: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 写入素材。若内容哈希已存在则返回已有 Asset（自然去重），并返回 `(asset, deduped)` 标志。
    pub fn put(&self, asset: Asset) -> (Asset, bool) {
        let mut hash_idx = self.hash_index.write();
        if let Some(existing_id) = hash_idx.get(&asset.content_hash) {
            let existing = self.assets.read().get(existing_id).cloned();
            if let Some(existing) = existing {
                debug!(hash = %asset.content_hash, "asset deduplicated");
                return (existing, true);
            }
        }
        let id = asset.id.clone();
        hash_idx.insert(asset.content_hash.clone(), id.clone());

        // 同步写入模拟 SQLite 表
        let row = serde_json::to_value(&asset).unwrap_or_default();
        self.table.write().push(row);

        self.assets.write().insert(id.clone(), asset.clone());
        (asset, false)
    }

    pub fn get(&self, id: &str) -> Option<Asset> {
        self.assets.read().get(id).cloned()
    }

    pub fn delete(&self, id: &str) -> Option<Asset> {
        let removed = self.assets.write().remove(id);
        if let Some(ref a) = removed {
            self.hash_index.write().remove(&a.content_hash);
            self.table.write().retain(|r| r["id"].as_str() != Some(id));
        }
        removed
    }

    pub fn list(&self) -> Vec<Asset> {
        self.assets.read().values().cloned().collect()
    }

    pub fn count(&self) -> usize {
        self.assets.read().len()
    }

    /// 返回模拟 SQLite 表的快照
    pub fn table_snapshot(&self) -> Vec<serde_json::Value> {
        self.table.read().clone()
    }

    /// 更新素材的 EXIF / 缩略图 / pHash
    pub fn update<F>(&self, id: &str, f: F) -> Option<Asset>
    where
        F: FnOnce(&mut Asset),
    {
        let mut assets = self.assets.write();
        let asset = assets.get_mut(id)?;
        f(asset);
        Some(asset.clone())
    }
}

// ============================================================================
// SubTask 3.5.2: 缩略图生成
// ============================================================================

/// 缩略图
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thumbnail {
    pub asset_id: AssetId,
    pub width: u32,
    pub height: u32,
    pub format: ThumbnailFormat,
    /// 缩略图内容哈希（mock：基于源内容哈希派生）
    pub thumb_hash: String,
    pub storage_path: String,
    pub generated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThumbnailFormat {
    WebP,
    Jpeg,
    Png,
}

/// 缩略图生成器（mock）
pub struct ThumbnailGenerator;

impl ThumbnailGenerator {
    /// 为素材生成缩略图。
    /// 简化：不调用真实 image / ffmpeg，仅基于源内容哈希派生确定性标识。
    /// 对于视频，模拟抽取关键帧；对于图片，模拟缩放。
    pub fn generate(asset: &Asset, target_width: u32) -> Thumbnail {
        let format = if asset.asset_type == AssetType::Image {
            // WebP 优先
            ThumbnailFormat::WebP
        } else {
            ThumbnailFormat::Jpeg
        };
        // 模拟 16:9 缩略图
        let height = (target_width as f32 * 9.0 / 16.0).round() as u32;
        let thumb_hash =
            content_hash_hex(format!("{}:{}", asset.content_hash, target_width).as_bytes());
        let path = format!("thumbnails/{}/{}", &asset.content_hash[..2], thumb_hash);
        info!(asset_id = %asset.id, "thumbnail generated (mock)");
        Thumbnail {
            asset_id: asset.id.clone(),
            width: target_width,
            height,
            format,
            thumb_hash,
            storage_path: path,
            generated_at: chrono::Utc::now(),
        }
    }
}

// ============================================================================
// SubTask 3.5.3: EXIF 元数据解析
// ============================================================================

/// EXIF 元数据
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExifMetadata {
    /// 拍摄时间（ISO 8601）
    pub capture_time: Option<String>,
    /// GPS 坐标
    pub gps: Option<GpsCoordinates>,
    /// 设备信息
    pub device: Option<DeviceInfo>,
    /// 方向（度）
    pub orientation: Option<f32>,
    /// 原始标签（键值对）
    pub tags: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GpsCoordinates {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceInfo {
    pub make: String,
    pub model: String,
    pub software: Option<String>,
}

/// EXIF 解析器（简化版）
///
/// 接受一个 `&[(tag, value)]` 简化标签结构，从中提取 datetime / GPS / device。
pub struct ExifParser;

impl ExifParser {
    /// 从原始 EXIF 标签列表解析
    pub fn parse(tags: &[(String, String)]) -> ExifMetadata {
        let mut meta = ExifMetadata::default();
        for (k, v) in tags {
            meta.tags.insert(k.clone(), v.clone());
        }

        // DateTimeOriginal / DateTime
        meta.capture_time = tags
            .iter()
            .find(|(k, _)| k == "DateTimeOriginal" || k == "DateTime")
            .map(|(_, v)| v.clone());

        // GPS
        let lat = tags
            .iter()
            .find(|(k, _)| k == "GPSLatitude")
            .map(|(_, v)| v);
        let lon = tags
            .iter()
            .find(|(k, _)| k == "GPSLongitude")
            .map(|(_, v)| v);
        let alt = tags
            .iter()
            .find(|(k, _)| k == "GPSAltitude")
            .map(|(_, v)| v);
        if let (Some(lat_s), Some(lon_s)) = (lat, lon) {
            if let (Ok(lat_f), Ok(lon_f)) = (lat_s.parse::<f64>(), lon_s.parse::<f64>()) {
                meta.gps = Some(GpsCoordinates {
                    latitude: lat_f,
                    longitude: lon_f,
                    altitude: alt.and_then(|s| s.parse::<f64>().ok()),
                });
            }
        }

        // Device
        let make = tags
            .iter()
            .find(|(k, _)| k == "Make")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        let model = tags
            .iter()
            .find(|(k, _)| k == "Model")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        let software = tags
            .iter()
            .find(|(k, _)| k == "Software")
            .map(|(_, v)| v.clone());
        if !make.is_empty() || !model.is_empty() {
            meta.device = Some(DeviceInfo {
                make,
                model,
                software,
            });
        }

        // Orientation
        meta.orientation = tags
            .iter()
            .find(|(k, _)| k == "Orientation")
            .and_then(|(_, v)| v.parse::<f32>().ok());

        meta
    }

    /// 序列化为 JSON 字符串（用于 SQLite 存储）
    pub fn to_json(meta: &ExifMetadata) -> String {
        serde_json::to_string(meta).unwrap_or_else(|_| "{}".to_string())
    }
}

// ============================================================================
// SubTask 3.5.4: 重复检测
// ============================================================================

/// 感知哈希（perceptual hash）
///
/// 简化实现：将图像数据下采样为 8x8 灰度网格，再二值化为 64-bit 平均哈希（aHash）。
/// 真实 pHash 使用 DCT，这里用平均值法近似，文档已说明。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PerceptualHash(pub u64);

impl PerceptualHash {
    /// 从「灰度像素列表」计算 aHash。
    /// 输入应为已下采样到 8x8 = 64 个灰度值（0-255）。
    pub fn from_grayscale_8x8(pixels: &[u8]) -> Self {
        assert_eq!(pixels.len(), 64, "expected 8x8 grayscale pixels");
        let avg = pixels.iter().map(|&p| p as u64).sum::<u64>() / 64;
        let mut hash: u64 = 0;
        for (i, &p) in pixels.iter().enumerate() {
            if (p as u64) > avg {
                hash |= 1 << i;
            }
        }
        PerceptualHash(hash)
    }

    /// 从任意图像数据「模拟」下采样并计算 aHash。
    /// 简化：直接取前 64 字节作为灰度采样（mock，不真实）。
    pub fn from_bytes(data: &[u8]) -> Self {
        let mut samples = [0u8; 64];
        for (i, slot) in samples.iter_mut().enumerate() {
            *slot = if i < data.len() { data[i] } else { 0 };
        }
        Self::from_grayscale_8x8(&samples)
    }

    /// 计算两个 pHash 的汉明距离
    pub fn hamming_distance(&self, other: &Self) -> u32 {
        (self.0 ^ other.0).count_ones()
    }

    /// 相似度（0.0 ~ 1.0），1.0 表示完全相同
    pub fn similarity(&self, other: &Self) -> f32 {
        let dist = self.hamming_distance(other);
        1.0 - (dist as f32 / 64.0)
    }
}

/// 重复检测结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    /// 重复素材 ID 列表
    pub asset_ids: Vec<AssetId>,
    /// 重复类型
    pub kind: DuplicateKind,
    /// 相似度（exact 时为 1.0）
    pub similarity: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DuplicateKind {
    /// 内容完全相同（SHA3-256 命中）
    Exact,
    /// 感知哈希相似
    Perceptual,
}

/// 重复检测器
pub struct DuplicateDetector {
    /// 相似度阈值（>= threshold 视为感知重复）
    pub phash_threshold: f32,
}

impl Default for DuplicateDetector {
    fn default() -> Self {
        Self {
            phash_threshold: 0.9,
        }
    }
}

impl DuplicateDetector {
    pub fn new(threshold: f32) -> Self {
        Self {
            phash_threshold: threshold,
        }
    }

    /// 检测精确重复（基于 content_hash）
    pub fn find_exact_duplicates(&self, assets: &[Asset]) -> Vec<DuplicateGroup> {
        let mut groups: HashMap<String, Vec<AssetId>> = HashMap::new();
        for a in assets {
            groups
                .entry(a.content_hash.clone())
                .or_default()
                .push(a.id.clone());
        }
        groups
            .into_iter()
            .filter(|(_, ids)| ids.len() > 1)
            .map(|(_, ids)| DuplicateGroup {
                asset_ids: ids,
                kind: DuplicateKind::Exact,
                similarity: 1.0,
            })
            .collect()
    }

    /// 检测感知重复（基于 pHash 汉明距离）
    pub fn find_perceptual_duplicates(&self, assets: &[Asset]) -> Vec<DuplicateGroup> {
        let mut groups: Vec<DuplicateGroup> = Vec::new();
        let mut visited: HashSet<AssetId> = HashSet::new();

        for (i, a) in assets.iter().enumerate() {
            if visited.contains(&a.id) {
                continue;
            }
            let pa = match &a.phash {
                Some(p) => p,
                None => continue,
            };
            let mut group_ids = vec![a.id.clone()];
            for b in assets.iter().skip(i + 1) {
                if visited.contains(&b.id) {
                    continue;
                }
                if let Some(pb) = &b.phash {
                    let sim = pa.similarity(pb);
                    if sim >= self.phash_threshold {
                        group_ids.push(b.id.clone());
                        visited.insert(b.id.clone());
                    }
                }
            }
            if group_ids.len() > 1 {
                visited.insert(a.id.clone());
                groups.push(DuplicateGroup {
                    asset_ids: group_ids,
                    kind: DuplicateKind::Perceptual,
                    similarity: self.phash_threshold,
                });
            }
        }
        groups
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn make_asset(name: &str, mime: &str, data: &[u8]) -> Asset {
        Asset::new(name, mime, data)
    }

    // --- Content-addressed storage ---

    #[test]
    fn test_content_hash_deterministic() {
        let a = content_hash_hex(b"hello");
        let b = content_hash_hex(b"hello");
        let c = content_hash_hex(b"world");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64); // SHA3-256 = 32 bytes = 64 hex chars
    }

    #[test]
    fn test_asset_id_is_content_hash() {
        let asset = make_asset("a.png", "image/png", b"data");
        assert_eq!(asset.id, asset.content_hash);
        assert_eq!(asset.asset_type, AssetType::Image);
    }

    #[test]
    fn test_asset_store_dedup() {
        let store = AssetStore::new();
        let a1 = make_asset("a.png", "image/png", b"identical content");
        let a2 = make_asset("b.png", "image/png", b"identical content");
        let (stored1, dedup1) = store.put(a1);
        let (stored2, dedup2) = store.put(a2);
        assert!(!dedup1);
        assert!(dedup2);
        assert_eq!(stored1.id, stored2.id);
        assert_eq!(store.count(), 1);
    }

    #[test]
    fn test_asset_store_distinct() {
        let store = AssetStore::new();
        store.put(make_asset("a.png", "image/png", b"content a"));
        store.put(make_asset("b.png", "image/png", b"content b"));
        assert_eq!(store.count(), 2);
    }

    #[test]
    fn test_asset_store_delete() {
        let store = AssetStore::new();
        let (a, _) = store.put(make_asset("a.png", "image/png", b"content a"));
        assert!(store.delete(&a.id).is_some());
        assert!(store.get(&a.id).is_none());
        assert_eq!(store.count(), 0);
    }

    #[test]
    fn test_asset_store_table_snapshot() {
        let store = AssetStore::new();
        store.put(make_asset("a.png", "image/png", b"content a"));
        let snap = store.table_snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0]["original_name"], "a.png");
    }

    #[test]
    fn test_asset_type_from_mime() {
        assert_eq!(AssetType::from_mime("image/png"), AssetType::Image);
        assert_eq!(AssetType::from_mime("video/mp4"), AssetType::Video);
        assert_eq!(AssetType::from_mime("audio/mpeg"), AssetType::Audio);
        assert_eq!(AssetType::from_mime("application/pdf"), AssetType::Document);
        assert_eq!(AssetType::from_mime("foo/bar"), AssetType::Other);
    }

    // --- Thumbnail ---

    #[test]
    fn test_thumbnail_image_uses_webp() {
        let asset = make_asset("a.png", "image/png", b"img data");
        let thumb = ThumbnailGenerator::generate(&asset, 256);
        assert_eq!(thumb.format, ThumbnailFormat::WebP);
        assert_eq!(thumb.width, 256);
        assert_eq!(thumb.height, 144); // 16:9
        assert!(thumb.storage_path.starts_with("thumbnails/"));
    }

    #[test]
    fn test_thumbnail_video_uses_jpeg() {
        let asset = make_asset("a.mp4", "video/mp4", b"video data");
        let thumb = ThumbnailGenerator::generate(&asset, 128);
        assert_eq!(thumb.format, ThumbnailFormat::Jpeg);
    }

    #[test]
    fn test_thumbnail_deterministic() {
        let asset = make_asset("a.png", "image/png", b"img data");
        let t1 = ThumbnailGenerator::generate(&asset, 256);
        let t2 = ThumbnailGenerator::generate(&asset, 256);
        assert_eq!(t1.thumb_hash, t2.thumb_hash);
    }

    // --- EXIF ---

    #[test]
    fn test_exif_parse_full() {
        let tags = vec![
            (
                "DateTimeOriginal".to_string(),
                "2024:01:15 10:30:00".to_string(),
            ),
            ("GPSLatitude".to_string(), "31.2304".to_string()),
            ("GPSLongitude".to_string(), "121.4737".to_string()),
            ("GPSAltitude".to_string(), "10.5".to_string()),
            ("Make".to_string(), "Canon".to_string()),
            ("Model".to_string(), "EOS R5".to_string()),
            ("Software".to_string(), "v1.0".to_string()),
            ("Orientation".to_string(), "1".to_string()),
        ];
        let meta = ExifParser::parse(&tags);
        assert_eq!(meta.capture_time.as_deref(), Some("2024:01:15 10:30:00"));
        let gps = meta.gps.unwrap();
        assert!((gps.latitude - 31.2304).abs() < 1e-6);
        assert!((gps.longitude - 121.4737).abs() < 1e-6);
        assert!((gps.altitude.unwrap() - 10.5).abs() < 1e-6);
        let dev = meta.device.unwrap();
        assert_eq!(dev.make, "Canon");
        assert_eq!(dev.model, "EOS R5");
        assert_eq!(dev.software.as_deref(), Some("v1.0"));
        assert_eq!(meta.orientation, Some(1.0));
    }

    #[test]
    fn test_exif_parse_empty() {
        let meta = ExifParser::parse(&[]);
        assert!(meta.capture_time.is_none());
        assert!(meta.gps.is_none());
        assert!(meta.device.is_none());
    }

    #[test]
    fn test_exif_partial_gps_ignored() {
        let tags = vec![("GPSLatitude".to_string(), "31.0".to_string())];
        let meta = ExifParser::parse(&tags);
        assert!(meta.gps.is_none());
    }

    #[test]
    fn test_exif_to_json() {
        let tags = vec![("Make".to_string(), "Apple".to_string())];
        let meta = ExifParser::parse(&tags);
        let json = ExifParser::to_json(&meta);
        assert!(json.contains("Apple"));
    }

    // --- pHash / Duplicate detection ---

    #[test]
    fn test_phash_from_grayscale_identical() {
        let pixels = [128u8; 64];
        let h1 = PerceptualHash::from_grayscale_8x8(&pixels);
        let h2 = PerceptualHash::from_grayscale_8x8(&pixels);
        assert_eq!(h1, h2);
        assert_eq!(h1.hamming_distance(&h2), 0);
        assert_eq!(h1.similarity(&h2), 1.0);
    }

    #[test]
    fn test_phash_similarity_in_range() {
        // 上升梯度 vs 下降梯度：结构完全相反，相似度应较低
        let mut p1 = [0u8; 64];
        let mut p2 = [0u8; 64];
        for i in 0..64 {
            p1[i] = (i * 4) as u8;
            p2[i] = 255 - (i * 4) as u8;
        }
        let h1 = PerceptualHash::from_grayscale_8x8(&p1);
        let h2 = PerceptualHash::from_grayscale_8x8(&p2);
        let sim = h1.similarity(&h2);
        assert!(sim >= 0.0 && sim <= 1.0);
        // 梯度方向相反 → 64 位全部不同 → 相似度 0
        assert!(sim < 0.6);
    }

    #[test]
    fn test_phash_from_bytes_deterministic() {
        let h1 = PerceptualHash::from_bytes(b"some image data");
        let h2 = PerceptualHash::from_bytes(b"some image data");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_duplicate_detector_exact() {
        let store = AssetStore::new();
        // 不同名字但相同内容 → 内容寻址会去重，store 只剩 1 个
        store.put(make_asset("a.png", "image/png", b"same"));
        store.put(make_asset("b.png", "image/png", b"same"));
        let assets = store.list();
        // 内容寻址去重后只剩 1 个，所以精确重复为 0 组
        assert_eq!(assets.len(), 1);
        let detector = DuplicateDetector::default();
        let groups = detector.find_exact_duplicates(&assets);
        assert_eq!(groups.len(), 0);
    }

    #[test]
    fn test_duplicate_detector_exact_with_clones() {
        // 构造两个 asset，强制相同 content_hash 但不同 id（绕过 store 去重）
        let mut a1 = make_asset("a.png", "image/png", b"same content");
        let mut a2 = make_asset("b.png", "image/png", b"same content");
        a1.id = Uuid::new_v4().to_string();
        a2.id = Uuid::new_v4().to_string();
        let detector = DuplicateDetector::default();
        let groups = detector.find_exact_duplicates(&[a1, a2]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].asset_ids.len(), 2);
        assert_eq!(groups[0].kind, DuplicateKind::Exact);
    }

    #[test]
    fn test_duplicate_detector_perceptual() {
        // 梯度图 + 微小扰动（翻转阈值附近 1 个像素）→ 仅 1 位不同
        let mut p1 = [0u8; 64];
        for i in 0..64 {
            p1[i] = (i * 4) as u8;
        }
        let mut p2 = p1;
        // p1[32] = 128，avg ≈ 126 → bit 32 原本为 1；改为 100 使其低于 avg
        p2[32] = 100;
        let h1 = PerceptualHash::from_grayscale_8x8(&p1);
        let h2 = PerceptualHash::from_grayscale_8x8(&p2);
        // 相似度应 >= 0.9（仅 1 位不同）
        assert!(h1.similarity(&h2) >= 0.9);

        let mut a1 = make_asset("a.png", "image/png", b"data1");
        let mut a2 = make_asset("b.png", "image/png", b"data2");
        a1.id = Uuid::new_v4().to_string();
        a2.id = Uuid::new_v4().to_string();
        a1.phash = Some(h1);
        a2.phash = Some(h2);
        let detector = DuplicateDetector::default();
        let groups = detector.find_perceptual_duplicates(&[a1, a2]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].kind, DuplicateKind::Perceptual);
    }

    #[test]
    fn test_duplicate_detector_no_perceptual_for_dissimilar() {
        // 上升梯度 vs 下降梯度 → 64 位全部不同 → 不相似
        let mut p1 = [0u8; 64];
        let mut p2 = [0u8; 64];
        for i in 0..64 {
            p1[i] = (i * 4) as u8;
            p2[i] = 255 - (i * 4) as u8;
        }
        let h1 = PerceptualHash::from_grayscale_8x8(&p1);
        let h2 = PerceptualHash::from_grayscale_8x8(&p2);
        let mut a1 = make_asset("a.png", "image/png", b"data1");
        let mut a2 = make_asset("b.png", "image/png", b"data2");
        a1.id = Uuid::new_v4().to_string();
        a2.id = Uuid::new_v4().to_string();
        a1.phash = Some(h1);
        a2.phash = Some(h2);
        let detector = DuplicateDetector::default();
        let groups = detector.find_perceptual_duplicates(&[a1, a2]);
        assert_eq!(groups.len(), 0);
    }

    #[test]
    fn test_asset_store_update() {
        let store = AssetStore::new();
        let (a, _) = store.put(make_asset("a.jpg", "image/jpeg", b"img"));
        let updated = store.update(&a.id, |asset| {
            asset.exif = Some(ExifMetadata::default());
        });
        assert!(updated.is_some());
        let reloaded = store.get(&a.id).unwrap();
        assert!(reloaded.exif.is_some());
    }

    #[test]
    fn test_hamming_distance_symmetric() {
        let h1 = PerceptualHash(0b1010_1010);
        let h2 = PerceptualHash(0b0101_0101);
        assert_eq!(h1.hamming_distance(&h2), h2.hamming_distance(&h1));
    }
}
