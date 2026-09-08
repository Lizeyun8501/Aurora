//! MCP 数据中枢 — 工具注册表（V23-I5 最小切片）
//!
//! 「协议是稳定层」（破局提案核心洞察）：本模块把 Aurora 暴露给
//! MCP 客户端（Claude Desktop / Cline 等）的工具契约**先冻结为
//! 数据**——工具名 / 输入 schema / 只读或写属性 / 所需权限级。
//!
//! V22.1 裁决落地（§5.2 修正：拆两层）:
//! - 本模块 = mcp-gateway 的契约层（稳定）
//! - 传输层（stdio/HTTP+SSE）后续接入 — 契约不变
//! - 外部写入必须经用户确认 → 由业务组件产生正规写事件
//!   （铁律 11: 网关不得直接落库 — 故本表**无写工具**，写路径
//!   一期全走用户侧 UI）
//!
//! 四工具全部只读（L1），写路径二期加 confirmation 流程后开放。

use serde::{Deserialize, Serialize};

/// MCP 工具权限级（对齐 V23-S1 权限三级 — L0 只读锚定 / L1 读+写用户可见 / L2 全权）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpPerm {
    /// 只读（一期全部工具此级）。
    L1,
}

/// MCP 工具契约（稳定数据 — 传输层无关）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpTool {
    pub name: &'static str,
    pub description: &'static str,
    /// JSON Schema（输入参数 — 冻结后仅可追加可选字段）。
    pub input_schema: serde_json::Value,
    pub perm: McpPerm,
    /// 对应 AppCore 能力（实现路由提示）。
    pub appcore_route: &'static str,
}

/// 全部对外工具（顺序即 tools/list 顺序 — 保持稳定）。
pub fn tool_registry() -> Vec<McpTool> {
    vec![
        McpTool {
            name: "search_notes",
            description: "中文分词全文搜索笔记（jieba + Tantivy; 口语化查询透传）",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "关键词或口语化查询" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 50 }
                },
                "required": ["query"]
            }),
            perm: McpPerm::L1,
            appcore_route: "search_notes",
        },
        McpTool {
            name: "read_note",
            description: "读取单篇笔记全文（含标题与更新时间）",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "note_id": { "type": "string" }
                },
                "required": ["note_id"]
            }),
            perm: McpPerm::L1,
            appcore_route: "get_note_content",
        },
        McpTool {
            name: "today_stats",
            description: "今日 GTD 统计（进行中/已完成/今日到期 — 任务投影聚合）",
            input_schema: serde_json::json!({ "type": "object", "properties": {} }),
            perm: McpPerm::L1,
            appcore_route: "today_view_stats",
        },
        McpTool {
            name: "agent_context",
            description: "获取当前工作现场（当前文档+选中块+反链场景+GTD — Agent 现场感知）",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "note_id": { "type": "string" },
                    "selected_block": { "type": ["string", "null"], "description": "当前选中块 ID（可选）" }
                },
                "required": ["note_id"]
            }),
            perm: McpPerm::L1,
            appcore_route: "agent_context",
        },
    ]
}

/// tools/list 响应（MCP 协议形状 — 字段名与官方 spec 对齐）。
pub fn tools_list_response() -> serde_json::Value {
    serde_json::json!({
        "tools": tool_registry()
            .iter()
            .map(|t| serde_json::json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": t.input_schema,
            }))
            .collect::<Vec<_>>()
    })
}

/// 路由解析: 工具名 → AppCore 能力（未注册工具 Err — 拒绝而非猜测）。
pub fn route_tool(name: &str) -> Result<&'static str, crate::Error> {
    tool_registry()
        .iter()
        .find(|t| t.name == name)
        .map(|t| t.appcore_route)
        .ok_or_else(|| {
            crate::Error::InvalidInput(format!("unknown mcp tool: {name} (not in registry)"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 契约冻结验证: 四工具、全只读、名称与路由一一对应。
    #[test]
    fn registry_shape_frozen() {
        let tools = tool_registry();
        assert_eq!(tools.len(), 4);
        assert!(tools.iter().all(|t| t.perm == McpPerm::L1), "一期全只读（铁律 11）");
        for t in &tools {
            assert_eq!(t.input_schema["type"], "object", "{name} schema 须 object", name = t.name);
            assert!(!t.appcore_route.is_empty());
        }
        // 名称路由一一对应
        assert_eq!(route_tool("search_notes").unwrap(), "search_notes");
        assert_eq!(route_tool("read_note").unwrap(), "get_note_content");
        assert_eq!(route_tool("today_stats").unwrap(), "today_view_stats");
        assert_eq!(route_tool("agent_context").unwrap(), "agent_context");
        // 未注册 → Err（拒绝而非猜测）
        assert!(route_tool("write_note").is_err(), "写工具一期不存在");
        assert!(route_tool("drop_table").is_err());
    }

    /// tools/list 响应形状（MCP spec: tools[].{name,description,inputSchema}）。
    #[test]
    fn tools_list_mcp_shape() {
        let resp = tools_list_response();
        let arr = resp["tools"].as_array().unwrap();
        assert_eq!(arr.len(), 4);
        for t in arr {
            assert!(t["name"].is_string());
            assert!(t["description"].is_string());
            assert!(t["inputSchema"].is_object());
            assert!(t.get("perm").is_none(), "协议形状不带内部字段");
        }
        // agent_context 的 selected_block 允许 null（可选块选择）
        let agent = arr.iter().find(|t| t["name"] == "agent_context").unwrap();
        assert_eq!(agent["inputSchema"]["properties"]["selected_block"]["type"][0], "string");
        assert_eq!(agent["inputSchema"]["properties"]["selected_block"]["type"][1], "null");
    }

    /// schema 冻结纪律: required 字段不可消失（追加可选字段合法）。
    #[test]
    fn schema_required_fields_present() {
        let tools = tool_registry();
        let search = tools.iter().find(|t| t.name == "search_notes").unwrap();
        assert_eq!(search.input_schema["required"], serde_json::json!(["query"]));
        let read = tools.iter().find(|t| t.name == "read_note").unwrap();
        assert_eq!(read.input_schema["required"], serde_json::json!(["note_id"]));
    }
}
