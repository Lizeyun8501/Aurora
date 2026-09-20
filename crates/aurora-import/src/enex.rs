//! ENEX（印象笔记导出 XML）流式解析。
//!
//! ENEX 结构（Evernote Export DTD 3）：`<en-export>` 根下多个 `<note>`，
//! 每 note 含 `title` / `content`（CDATA 包裹的 ENML 文档）/ `created` /
//! `updated` / `tag*` / `resource*`（`data@encoding=base64@hash` + `mime` +
//! `resource-attributes/file-name`）。
//!
//! 解析用 quick-xml 流式事件（ENEX 文件可达数百 MB，避免 DOM 全量驻留）。
//! 任何单 note 解析错误都作为该 note 的失败返回，不影响其余 note。

use base64::Engine as _;
use quick_xml::events::Event;
use quick_xml::Reader;

/// ENEX 中解析出的单个原始笔记（ENML 未转换）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawEnexNote {
    /// 笔记标题（`<title>`；缺失时为空串，由调用方兜底）。
    pub title: String,
    /// `<content>` CDATA 内的完整 ENML 文档。
    pub content: String,
    /// `<created>`（Evernote 时间戳格式 `20240101T000000Z`）。
    pub created: Option<String>,
    /// `<updated>`。
    pub updated: Option<String>,
    /// `<tag>` 列表。
    pub tags: Vec<String>,
    /// `<resource>` 列表。
    pub resources: Vec<RawResource>,
}

/// ENEX 中的资源（base64 未解码）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawResource {
    /// `data@hash`（hex 摘要，ENML `en-media@hash` 引用键）。
    pub hash: String,
    /// MIME 类型。
    pub mime: String,
    /// base64 文本（含换行，解码时忽略）。
    pub data_base64: String,
    /// 原始文件名（`resource-attributes/file-name`）。
    pub file_name: Option<String>,
}

impl RawResource {
    /// base64 解码（容忍换行/空白；失败返回原因）。
    pub fn decode(&self) -> Result<Vec<u8>, String> {
        base64::engine::general_purpose::STANDARD
            .decode(
                self.data_base64
                    .split_ascii_whitespace()
                    .collect::<String>(),
            )
            .map_err(|e| format!("base64 解码失败 (hash={}): {e}", self.hash))
    }
}

/// 解析 ENEX 文本为原始笔记列表。
///
/// # Errors
/// XML 结构性损坏（无法闭合/嵌套错乱）返回首因错误；
/// 单 note 内字段异常不致失败（空 title 等由调用方兜底）。
pub fn parse_enex(xml: &str) -> Result<Vec<RawEnexNote>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);

    let mut notes = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.local_name().as_ref() == b"note" => {
                let note = parse_note(&mut reader)
                    .map_err(|e| format!("note #{} 解析失败: {e}", notes.len() + 1))?;
                notes.push(note);
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(format!("ENEX XML 读取失败: {e}")),
        }
        buf.clear();
    }
    Ok(notes)
}

/// 解析单个 `<note>`（光标已消费 `<note>` Start 事件）。
fn parse_note(reader: &mut Reader<&[u8]>) -> Result<RawEnexNote, String> {
    let mut note = RawEnexNote {
        title: String::new(),
        content: String::new(),
        created: None,
        updated: None,
        tags: Vec::new(),
        resources: Vec::new(),
    };
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.local_name().as_ref() {
                b"title" => note.title = read_text(reader)?,
                b"content" => note.content = read_cdata(reader)?,
                b"created" => note.created = Some(read_text(reader)?),
                b"updated" => note.updated = Some(read_text(reader)?),
                b"tag" => note.tags.push(read_text(reader)?),
                b"resource" => {
                    let r = parse_resource(reader).map_err(|e| format!("resource: {e}"))?;
                    note.resources.push(r);
                }
                // note-attributes 等其他子树：整体跳过（含任意嵌套）
                _ => skip_element(reader)?,
            },
            Ok(Event::End(e)) if e.local_name().as_ref() == b"note" => {
                return Ok(note);
            }
            Ok(Event::Eof) => return Err("note 未闭合（遇到 EOF）".to_string()),
            Ok(_) => {}
            Err(e) => return Err(e.to_string()),
        }
        buf.clear();
    }
}

/// 解析单个 `<resource>`（光标已消费 Start）。
fn parse_resource(reader: &mut Reader<&[u8]>) -> Result<RawResource, String> {
    let mut hash = String::new();
    let mut mime = String::new();
    let mut data_base64 = String::new();
    let mut file_name: Option<String> = None;
    let mut in_attributes = false;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                match e.local_name().as_ref() {
                    // data@hash 在 Start 事件属性上
                    b"data" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"hash" {
                                hash = attr
                                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                    .map_err(|e| e.to_string())?
                                    .into_owned();
                            }
                        }
                        data_base64 = read_text(reader)?;
                    }
                    b"mime" => mime = read_text(reader)?,
                    b"resource-attributes" => in_attributes = true,
                    b"file-name" if in_attributes => file_name = Some(read_text(reader)?),
                    _ => skip_element(reader)?,
                }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"resource" => {
                    return Ok(RawResource {
                        hash,
                        mime,
                        data_base64,
                        file_name,
                    });
                }
                b"resource-attributes" => in_attributes = false,
                _ => {}
            },
            Ok(Event::Eof) => return Err("resource 未闭合".to_string()),
            Ok(_) => {}
            Err(e) => return Err(e.to_string()),
        }
        buf.clear();
    }
}

/// 读当前简单元素的全部文本（消费 Start..End）。
fn read_text(reader: &mut Reader<&[u8]>) -> Result<String, String> {
    let mut out = String::new();
    let mut buf = Vec::new();
    let mut warnings = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Text(e)) => out.push_str(&e.decode().map_err(|e| e.to_string())?),
            Ok(Event::CData(e)) => out.push_str(&e.decode().map_err(|e| e.to_string())?),
            Ok(Event::GeneralRef(e)) => {
                crate::enml::push_general_ref(&mut out, &e, &mut warnings)?;
            }
            Ok(Event::End(_)) => return Ok(out),
            Ok(Event::Eof) => return Err("元素未闭合（EOF）".to_string()),
            Ok(_) => {}
            Err(e) => return Err(e.to_string()),
        }
        buf.clear();
    }
}

/// 读 `<content>`：CDATA 优先，普通文本兜底（消费 Start..End）。
fn read_cdata(reader: &mut Reader<&[u8]>) -> Result<String, String> {
    let mut out = String::new();
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::CData(e)) => out.push_str(&e.decode().map_err(|e| e.to_string())?),
            Ok(Event::Text(e)) => out.push_str(&e.decode().map_err(|e| e.to_string())?),
            Ok(Event::End(_)) => return Ok(out),
            Ok(Event::Eof) => return Err("content 未闭合（EOF）".to_string()),
            Ok(_) => {}
            Err(e) => return Err(e.to_string()),
        }
        buf.clear();
    }
}

/// 跳过整个元素（光标已消费 Start；容忍任意嵌套深度）。
fn skip_element(reader: &mut Reader<&[u8]>) -> Result<(), String> {
    let mut depth = 1usize;
    let mut buf = Vec::new();
    while depth > 0 {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(_)) => depth += 1,
            Ok(Event::End(_)) => depth -= 1,
            Ok(Event::Eof) => return Err("元素未闭合（EOF）".to_string()),
            Ok(_) => {}
            Err(e) => return Err(e.to_string()),
        }
        buf.clear();
    }
    Ok(())
}
