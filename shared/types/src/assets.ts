/**
 * Asset Library domain types.
 * Mirrors `crates/aurora-core/src/l3_domain/asset_library.rs`.
 */

/** Asset unique identifier — content-addressed (SHA3-256 hex). Mirrors `AssetId`. */
export type AssetId = string;

/** Perceptual hash (aHash, 64-bit). Stored as a string for safe u64 representation. */
export type PerceptualHash = string;

/** Asset type (mirrors `AssetType`, serde `snake_case`). */
export type AssetType = 'image' | 'video' | 'audio' | 'document' | 'other';

/** Thumbnail format (mirrors `ThumbnailFormat`, serde `snake_case`). */
export type ThumbnailFormat = 'web_p' | 'jpeg' | 'png';

/** Duplicate kind (mirrors `DuplicateKind`, serde `snake_case`). */
export type DuplicateKind = 'exact' | 'perceptual';

/** GPS coordinates (mirrors `GpsCoordinates`). */
export interface GpsCoordinates {
  latitude: number;
  longitude: number;
  altitude: number | null;
}

/** Device info (mirrors `DeviceInfo`). */
export interface DeviceInfo {
  make: string;
  model: string;
  software: string | null;
}

/** EXIF metadata (mirrors `ExifMetadata`). */
export interface ExifData {
  /** Capture time (ISO 8601). */
  capture_time: string | null;
  gps: GpsCoordinates | null;
  device: DeviceInfo | null;
  /** Orientation in degrees. */
  orientation: number | null;
  /** Raw EXIF tags. */
  tags: Record<string, string>;
}

/**
 * Asset metadata bundle — alias for the EXIF/metadata payload attached to an asset.
 * Kept distinct from `ExifData` to allow future metadata extensions.
 */
export interface AssetMetadata {
  exif: ExifData | null;
  /** Width in pixels (when applicable). */
  width: number | null;
  /** Height in pixels (when applicable). */
  height: number | null;
  /** Duration in seconds (for video/audio). */
  duration_seconds: number | null;
}

/** Thumbnail (mirrors `Thumbnail`). */
export interface Thumbnail {
  asset_id: AssetId;
  width: number;
  height: number;
  format: ThumbnailFormat;
  /** Thumbnail content hash. */
  thumb_hash: string;
  storage_path: string;
  generated_at: string;
}

/** Asset structure (mirrors `Asset`). */
export interface Asset {
  id: AssetId;
  original_name: string;
  mime_type: string;
  asset_type: AssetType;
  size_bytes: number;
  /** Content hash (SHA3-256 hex). */
  content_hash: string;
  /** Storage path (relative path or URI). */
  storage_path: string;
  created_at: string;
  exif: ExifData | null;
  thumbnail: Thumbnail | null;
  /** Perceptual hash (for duplicate detection). */
  phash: PerceptualHash | null;
}

/** Duplicate detection result (mirrors `DuplicateGroup`). */
export interface DuplicateGroup {
  asset_ids: AssetId[];
  kind: DuplicateKind;
  /** Similarity (1.0 for exact). */
  similarity: number;
}
