//! 层间通信数据序列化规范
//!
//! - 核心层内部：使用 Rust 原生类型，跨进程使用 bincode 或 MessagePack 序列化
//! - 核心层与视图层：使用 JSON，复杂二进制数据使用 Base64 编码或 Blob URL
//! - 网络传输：使用 protobuf 或 MessagePack，减少带宽占用
//! - 持久化存储：CRDT 数据使用 Loro 二进制格式，元数据使用 SQLite，大文件使用对象存储

use serde::{Deserialize, Serialize};

/// 序列化格式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerializationFormat {
    /// JSON - 核心层与视图层通信
    Json,
    /// Bincode - 核心层内部跨进程
    Bincode,
    /// MessagePack - 网络传输
    MessagePack,
    /// Protocol Buffers - 网络传输（未来）
    Protobuf,
}

/// 统一序列化入口
pub fn serialize<T: Serialize>(
    value: &T,
    format: SerializationFormat,
) -> Result<Vec<u8>, crate::Error> {
    match format {
        SerializationFormat::Json => serde_json::to_vec(value).map_err(crate::Error::Serialization),
        SerializationFormat::Bincode => bincode::serialize(value)
            .map_err(|e| crate::Error::Internal(format!("Bincode error: {}", e))),
        SerializationFormat::MessagePack => {
            // MessagePack not yet implemented, fallback to JSON
            serde_json::to_vec(value).map_err(crate::Error::Serialization)
        }
        SerializationFormat::Protobuf => {
            // Protobuf not yet implemented, fallback to JSON
            serde_json::to_vec(value).map_err(crate::Error::Serialization)
        }
    }
}

/// 统一反序列化入口
pub fn deserialize<'de, T: Deserialize<'de>>(
    bytes: &'de [u8],
    format: SerializationFormat,
) -> Result<T, crate::Error> {
    match format {
        SerializationFormat::Json => {
            serde_json::from_slice(bytes).map_err(crate::Error::Serialization)
        }
        SerializationFormat::Bincode => bincode::deserialize(bytes)
            .map_err(|e| crate::Error::Internal(format!("Bincode error: {}", e))),
        SerializationFormat::MessagePack => {
            serde_json::from_slice(bytes).map_err(crate::Error::Serialization)
        }
        SerializationFormat::Protobuf => {
            serde_json::from_slice(bytes).map_err(crate::Error::Serialization)
        }
    }
}

/// Base64 编码二进制数据（用于 JSON 传输）
pub fn encode_base64(data: &[u8]) -> String {
    use std::fmt::Write;
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        write!(result, "{}", CHARS[((triple >> 18) & 0x3F) as usize]).unwrap();
        write!(result, "{}", CHARS[((triple >> 12) & 0x3F) as usize]).unwrap();
        if chunk.len() > 1 {
            write!(result, "{}", CHARS[((triple >> 6) & 0x3F) as usize]).unwrap();
        } else {
            write!(result, "=").unwrap();
        }
        if chunk.len() > 2 {
            write!(result, "{}", CHARS[(triple & 0x3F) as usize]).unwrap();
        } else {
            write!(result, "=").unwrap();
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestData {
        name: String,
        value: i32,
    }

    #[test]
    fn test_json_roundtrip() {
        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };
        let bytes = serialize(&data, SerializationFormat::Json).unwrap();
        let deserialized: TestData = deserialize(&bytes, SerializationFormat::Json).unwrap();
        assert_eq!(data, deserialized);
    }

    #[test]
    fn test_bincode_roundtrip() {
        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };
        let bytes = serialize(&data, SerializationFormat::Bincode).unwrap();
        let deserialized: TestData = deserialize(&bytes, SerializationFormat::Bincode).unwrap();
        assert_eq!(data, deserialized);
    }

    #[test]
    fn test_base64_encoding() {
        let data = b"Hello, Aurora!";
        let encoded = encode_base64(data);
        assert!(!encoded.is_empty());
    }
}
