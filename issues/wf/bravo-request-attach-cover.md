# bravo-request: cmd_attach_cover —— 封面自定义图附件写入命令（C-04 依赖）

> 提请: Bravo → Alpha ｜ 2026-11-18 ｜ 域: apps/desktop/src-tauri（Alpha 领地——Bravo 不硬改）
> 卡: C-04 封面任务③「自定义图片（走附件与加密通道）」

## 缺口
桌面无 JS 可调的附件**写**命令（现有仅 read/export/open 三读面）。
core WritePath 已有 `attach_to_note(ctx, note_id, file_name, mime, data)`
（sealed 落库+内容寻址去重——crates/aurora-core/src/write_path/attach.rs:11）,
缺 IPC 薄封装。

## 请求
```rust
#[tauri::command]
pub async fn cmd_attach_cover(note_id: String, file_name: String, mime: String, data_base64: String)
  -> Result<serde_json::Value, String>  // { attachmentId }
// ≤2MB 守卫;attach_to_note 唯一入口;失败 fail-closed 透传
```

## 消费面（已落——C-04 d/s 特性探测就绪）
- DesktopShell CoverHeader `onCustomCover` prop → invoke('cmd_attach_cover')
- 展示读复用 cmd_read_attachment（封面档 attachment:// scheme 渲染挂点已留）
- 命令落地前 UI 禁用态+title 披露（非静默假成功——E15 吞病纪律）

## 边界预写死
- 仅封面语义（单图 ≤2MB）;不做通用上传器（导入器已有大盘）
- attachment_id 白名单校验复用 validate_attachment_id（注入防护不旁路）
