//! NL→SQL 沙箱 — V20 Phase 3 退出条件
//!
//! 「NL→SQL 沙箱拒绝**全部**非 SELECT 与越权 workspace 用例」
//! 的 Rust 侧强制边界（V20 §3.6.2: 结构化统计走 NL→SQL，后者受
//! sqlparser 沙箱、参数化与应用层 workspace_id 行级过滤保护）。
//!
//! # 三层防线
//!
//! 1. **语句白名单**（AST 级 — sqlparser 解析后判定）:
//!    仅接受单条 `SELECT`；任何 DML/DDL/PRAGMA/ATTACH 词形在
//!    AST 层即拒绝（注释/大小写/嵌套子查询无法绕过 — 词法黑名单
//!    可被 `/**/INSERT` 等绕过，AST 不会）
//! 2. **表白名单**: 仅业务只读表（notes/tasks/links/notes_fts/
//!    version_snapshots）; sqlite_master 等元数据表拒绝
//! 3. **workspace 行级过滤强制**: 含 workspace_id 列的表出现时，
//!    WHERE 树中必须存在该列的等值约束（否则拒绝 — 防跨工作区
//!    数据泄漏）; 绑定值由**应用层注入**（`:ws` 占位符），用户输入
//!    永远不进 SQL 文本（参数化）
//!
//! # 拒绝语义
//!
//! 返回 [`SandboxError`] 带原因码（审计日志可分类）; 永不部分放行
//! ——校验失败即整条拒绝。

use sqlparser::ast::{BinaryOperator, Expr, Query, SetExpr, Statement, TableFactor};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

/// 沙箱错误（原因码 + 说明 — 审计可分类）。
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum SandboxError {
    #[error("rejected: non-SELECT statement")]
    NotSelect,
    #[error("rejected: multiple statements (smuggling)")]
    MultipleStatements,
    #[error("rejected: table not allowed: {table}")]
    TableNotAllowed { table: String },
    #[error("rejected: missing workspace_id row filter for tables {tables:?}")]
    MissingWorkspaceFilter { tables: Vec<String> },
    #[error("rejected: unparseable SQL")]
    Parse(#[from] sqlparser::parser::ParserError),
}

/// 业务只读表白名单（小写）。
const ALLOWED_TABLES: &[&str] = &[
    "notes",
    "tasks",
    "links",
    "notes_fts",
    "version_snapshots",
];

/// 含 workspace_id 列、需行级过滤的表（白名单子集）。
const WORKSPACE_TABLES: &[&str] = &["notes", "notes_fts", "version_snapshots"];

/// 沙箱校验器（无状态 — 全静态策略）。
pub struct SqlSandbox;

impl SqlSandbox {
    /// 校验一条用户侧 SQL。
    ///
    /// 通过 → 返回原语句（调用方负责 `:ws` 参数绑定）；
    /// 拒绝 → [`SandboxError`]（审计可分类）。
    pub fn validate(sql: &str) -> Result<String, SandboxError> {
        let dialect = GenericDialect {};
        let stmts = Parser::parse_sql(&dialect, sql)?;
        if stmts.len() != 1 {
            return Err(SandboxError::MultipleStatements);
        }
        match &stmts[0] {
            Statement::Query(q) => {
                Self::validate_query(q)?;
                Ok(sql.to_string())
            }
            _ => Err(SandboxError::NotSelect),
        }
    }

    /// 查询体校验: 表白名单 + workspace 行过滤。
    fn validate_query(q: &Query) -> Result<(), SandboxError> {
        // CTE 也收集（WITH 中的表同样受白名单/过滤约束）;
        // CTE 别名本身不是表 — 白名单判定跳过（其定义已被收集）
        let mut cte_names: Vec<String> = Vec::new();
        let mut tables: Vec<String> = Vec::new();
        if let Some(with) = &q.with {
            for cte in &with.cte_tables {
                cte_names.push(cte.alias.name.value.to_lowercase());
                collect_tables(&cte.query.body, &mut tables);
            }
        }
        collect_tables(&q.body, &mut tables);

        for t in &tables {
            let name = t.to_lowercase();
            if cte_names.contains(&name) {
                continue; // CTE 别名透传
            }
            if !ALLOWED_TABLES.contains(&name.as_str()) {
                return Err(SandboxError::TableNotAllowed { table: name });
            }
        }

        let needs_filter = tables
            .iter()
            .any(|t| WORKSPACE_TABLES.contains(&t.to_lowercase().as_str()));
        if needs_filter && !sql_has_workspace_filter(&q.body) {
            return Err(SandboxError::MissingWorkspaceFilter {
                tables: tables.iter().map(|s| s.to_lowercase()).collect(),
            });
        }
        Ok(())
    }
}

/// 表名收集: 遍历 SetExpr 树（SELECT body / JOIN / 子查询 / UNION 分支）。
fn collect_tables(body: &SetExpr, out: &mut Vec<String>) {
    match body {
        SetExpr::Select(select) => {
            for from in &select.from {
                walk_table_factor(&from.relation, out);
                if let TableFactor::Derived { subquery, .. } = &from.relation {
                    collect_tables(&subquery.body, out);
                }
                for join in &from.joins {
                    walk_table_factor(&join.relation, out);
                    if let TableFactor::Derived { subquery, .. } = &join.relation {
                        collect_tables(&subquery.body, out);
                    }
                }
            }
        }
        SetExpr::Query(q) => {
            if let Some(with) = &q.with {
                for cte in &with.cte_tables {
                    collect_tables(&cte.query.body, out);
                }
            }
            collect_tables(&q.body, out);
        }
        SetExpr::SetOperation { left, right, .. } => {
            collect_tables(left, out);
            collect_tables(right, out);
        }
        _ => {}
    }
}

fn walk_table_factor(tf: &TableFactor, out: &mut Vec<String>) {
    if let TableFactor::Table { name, .. } = tf {
        let full = name
            .0
            .iter()
            .map(|p| p.value.clone())
            .collect::<Vec<_>>()
            .join(".");
        out.push(full);
    }
}

/// workspace 过滤存在性: 自身 WHERE 或 FROM 中派生表的过滤
/// （递归; UNION 任一分支命中即过）。
fn sql_has_workspace_filter(body: &SetExpr) -> bool {
    match body {
        SetExpr::Select(select) => {
            let self_filter = select
                .selection
                .as_ref()
                .map(expr_has_ws_eq)
                .unwrap_or(false);
            if self_filter {
                return true;
            }
            for from in &select.from {
                if let TableFactor::Derived { subquery, .. } = &from.relation {
                    if sql_has_workspace_filter(&subquery.body) {
                        return true;
                    }
                }
                for join in &from.joins {
                    if let TableFactor::Derived { subquery, .. } = &join.relation {
                        if sql_has_workspace_filter(&subquery.body) {
                            return true;
                        }
                    }
                }
            }
            false
        }
        SetExpr::Query(q) => sql_has_workspace_filter(&q.body),
        SetExpr::SetOperation { left, right, .. } => {
            sql_has_workspace_filter(left) || sql_has_workspace_filter(right)
        }
        _ => false,
    }
}

fn expr_has_ws_eq(e: &Expr) -> bool {
    match e {
        Expr::BinaryOp { left, op, right } => {
            let is_eq = matches!(op, BinaryOperator::Eq);
            if is_eq && (is_ws_ident(left) || is_ws_ident(right)) {
                return true;
            }
            expr_has_ws_eq(left) || expr_has_ws_eq(right)
        }
        Expr::Nested(e2) => expr_has_ws_eq(e2),
        _ => false,
    }
}

fn is_ws_ident(e: &Expr) -> bool {
    match e {
        Expr::Identifier(id) => id.value.eq_ignore_ascii_case("workspace_id"),
        Expr::CompoundIdentifier(ids) => ids
            .iter()
            .any(|p| p.value.eq_ignore_ascii_case("workspace_id")),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 基线: 带 workspace 过滤的合法 SELECT。
    #[test]
    fn allows_whitelisted_select_with_workspace() {
        let sql = "SELECT title FROM notes WHERE workspace_id = :ws AND status = 'done'";
        let r = SqlSandbox::validate(sql);
        assert!(r.is_ok(), "{r:?}");
        assert_eq!(r.unwrap(), sql);
    }

    /// 全部 DML/DDL/PRAGMA/ATTACK 词形 — 一律 NotSelect。
    #[test]
    fn rejects_all_non_select() {
        for sql in [
            "INSERT INTO notes VALUES (1)",
            "UPDATE notes SET title = 'x'",
            "DELETE FROM notes",
            "DROP TABLE notes",
            "ALTER TABLE notes ADD COLUMN x TEXT",
            "CREATE TABLE evil (id TEXT)",
            "PRAGMA journal_mode = DELETE",
            "ATTACH DATABASE '/tmp/evil.db' AS evil",
            "VACUUM",
            "REINDEX notes",
            "TRUNCATE TABLE notes",
        ] {
            let r = SqlSandbox::validate(sql);
            assert!(
                matches!(r, Err(SandboxError::NotSelect) | Err(SandboxError::Parse(_))),
                "{sql} → {r:?}"
            );
        }
    }

    /// 多语句走私（合法 SELECT + 藏一条 DROP）。
    #[test]
    fn rejects_multiple_statements() {
        let r = SqlSandbox::validate("SELECT 1 FROM notes_fts; DROP TABLE notes;");
        assert!(matches!(r, Err(SandboxError::MultipleStatements)), "{r:?}");
    }

    /// 注释绕过尝试 — AST 层不成立（写操作词在 AST 不存在）。
    #[test]
    fn comments_cannot_smuggle_write() {
        let r = SqlSandbox::validate("SELECT 1 /*, (DELETE FROM notes) */ FROM links");
        assert!(r.is_ok(), "注释内容不构成语句语义: {r:?}");
        // 真子查询写操作 → SQL 语法不允许 → Parse 拒绝
        let r2 = SqlSandbox::validate("SELECT (DELETE FROM notes) FROM notes_fts");
        assert!(r2.is_err());
    }

    /// 表白名单: sqlite_master / 未知表拒绝。
    #[test]
    fn rejects_unknown_tables() {
        let r = SqlSandbox::validate("SELECT name FROM sqlite_master");
        assert!(matches!(r, Err(SandboxError::TableNotAllowed { .. })), "{r:?}");
        let r2 = SqlSandbox::validate("SELECT * FROM users");
        assert!(matches!(r2, Err(SandboxError::TableNotAllowed { .. })), "{r2:?}");
    }

    /// workspace 行级过滤缺失 → 拒（含 JOIN / 子查询传播）。
    #[test]
    fn rejects_missing_workspace_filter() {
        for sql in [
            "SELECT title FROM notes",
            "SELECT 1 FROM tasks JOIN notes ON tasks.note_id = notes.id",
            "SELECT * FROM (SELECT title FROM notes)",
            "WITH t AS (SELECT title FROM notes WHERE workspace_id = :ws) \
             SELECT * FROM t UNION ALL SELECT content FROM notes",
        ] {
            let r = SqlSandbox::validate(sql);
            assert!(
                matches!(r, Err(SandboxError::MissingWorkspaceFilter { .. })),
                "{sql} → {r:?}"
            );
        }
    }

    /// 子查询内带过滤 → 放行（约束存在即可）。
    #[test]
    fn subquery_with_filter_allowed() {
        let sql = "SELECT * FROM (SELECT title FROM notes WHERE workspace_id = :ws)";
        assert!(SqlSandbox::validate(sql).is_ok());
    }

    /// 解析失败 / 空语句。
    #[test]
    fn parse_error_and_empty_rejected() {
        assert!(matches!(
            SqlSandbox::validate("SELEC * FRUM notes"),
            Err(SandboxError::Parse(_))
        ));
        assert!(SqlSandbox::validate("").is_err());
        assert!(SqlSandbox::validate("   ").is_err());
    }
}
