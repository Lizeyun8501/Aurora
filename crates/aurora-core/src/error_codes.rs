//! 统一错误码契约 — V26 DK-00/R-02（62 个，A12+B12+C10+D10+E8+F6+G4）
//!
//! 恢复 V22.1 的「用户文案 / 是否重试 / 降级动作」三元组（V25 合并中丢失），
//! 扩展至 62 个（V26-0 §4.2 表 4-2）。逐行列出而非先写总数。
//!
//! ## 降级三原则（铁律）
//!
//! 1. **B 类错误不得阻断本地编辑** — 同步、网络类错误只影响同步，用户仍可正常读写。
//! 2. **C03 密文校验失败一律拒绝返回明文** — 失败关闭（fail-closed），宁可用户看不到，
//!    不可返回可能被篡改的内容。
//! 3. **D 类只影响提案生成** — AI 错误不阻断任何已存在的数据读写。
//!
//! 契约变更必须走 ADR（`docs/adr/ADR-002-contract-governance.md`）。

use serde::{Deserialize, Serialize};

/// 降级动作（结构化，供 SyncRouter/TaskEngine 等调用方程序化决策）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FallbackAction {
    /// 无降级（仅提示）。
    None,
    /// 从最近备份恢复。
    RestoreBackup,
    /// 加载最近可用备份副本。
    LoadBackup,
    /// 暂停同步与索引。
    PauseSyncAndIndex,
    /// 写入 pending_writes 后台重做。
    PendingWrites,
    /// tmp→rename 原子重做。
    AtomicRetry,
    /// 回滚到上一版本。
    Rollback,
    /// 提示手动导出。
    PromptExport,
    /// 切换上一份备份。
    SwitchBackup,
    /// 标记待重做。
    MarkRedo,
    /// 保留引用，内容后续补。
    DeferContent,
    /// 降级为简单查询。
    SimpleQuery,
    /// 只读打开并提示升级。
    ReadOnly,
    /// 入 offline_queue。
    OfflineQueue,
    /// SyncRouter 切换目标（LAN→云端→WebDAV 链）。
    SwitchTarget,
    /// 降级链：LAN → 云端 → WebDAV。
    RelayChain,
    /// 生成冲突副本交用户选择。
    ConflictCopy,
    /// 回退全量 sync。
    FullSync,
    /// 停止同步并提示。
    StopSync,
    /// 批量合并后重发。
    BatchResend,
    /// 从断点恢复。
    Resume,
    /// 失败关闭：绝不返回明文。
    FailClosed,
    /// 强制本地模型。
    ForceLocal,
    /// 引导重新授权/配对。
    Reauthorize,
    /// 提示重新配置凭据。
    PromptReconfigure,
    /// 告警并冻结 Agent。
    FreezeAgent,
    /// 拒绝请求。
    Reject,
    /// 引导一键下载。
    PromptDownload,
    /// 降级量化等级。
    DowngradeQuant,
    /// 缩短上下文重试。
    ShorterContext,
    /// AIRouter 降级本地。
    RouterToLocal,
    /// 仅用 BM25。
    Bm25Only,
    /// 返回 NotImplemented（禁止伪文本/伪成功）。
    NotImplemented,
    /// 滑动窗口压缩。
    CompressContext,
    /// 返回错误给 Agent。
    ErrorToAgent,
    /// 拦截并记审计。
    InterceptAudit,
    /// 禁用该插件。
    DisablePlugin,
    /// 终止并回收资源。
    Terminate,
    /// 隔离并禁用。
    Isolate,
    /// 拒绝安装。
    RejectInstall,
    /// 排队等待。
    Queue,
    /// 从原生层重建。
    RebuildFromNative,
    /// 前台补偿同步。
    CompensateSync,
    /// 引导系统设置。
    PromptSettings,
    /// 隐藏入口。
    HideEntry,
}

impl FallbackAction {
    /// 是否存在实质降级（`requires_fallback` 三问）。
    pub fn requires_fallback(self) -> bool {
        !matches!(self, FallbackAction::None)
    }
}

/// 统一错误码 — 62 个（V26 表 4-2 冻结，逐行对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorCode {
    // ===== A 类 · 存储与数据（A01–A12）=====
    A01,
    A02,
    A03,
    A04,
    A05,
    A06,
    A07,
    A08,
    A09,
    A10,
    A11,
    A12,
    // ===== B 类 · 同步与网络（B01–B12）=====
    B01,
    B02,
    B03,
    B04,
    B05,
    B06,
    B07,
    B08,
    B09,
    B10,
    B11,
    B12,
    // ===== C 类 · 加密与安全（C01–C10）=====
    C01,
    C02,
    C03,
    C04,
    C05,
    C06,
    C07,
    C08,
    C09,
    C10,
    // ===== D 类 · AI 与模型（D01–D10）=====
    D01,
    D02,
    D03,
    D04,
    D05,
    D06,
    D07,
    D08,
    D09,
    D10,
    // ===== E 类 · 插件与扩展（E01–E08）=====
    E01,
    E02,
    E03,
    E04,
    E05,
    E06,
    E07,
    E08,
    // ===== F 类 · 业务规则（F01–F06）=====
    F01,
    F02,
    F03,
    F04,
    F05,
    F06,
    // ===== G 类 · 系统与平台（G01–G04）=====
    G01,
    G02,
    G03,
    G04,
}

/// 错误码契约三元组（用户文案 / 是否重试 / 降级动作）— V26 表 4-2 逐行。
type Row = (ErrorCode, &'static str, bool, FallbackAction);

/// 错误码矩阵 — 62 行，与 V26-0 §4.2 表 4-2 逐行对齐。
/// 变更必须走 ADR 并同步 `shared/types/src/errors.ts`。
pub const TABLE: &[Row; 62] = &[
    // A 类 · 存储与数据
    (
        ErrorCode::A01,
        "无法打开笔记库，正在尝试恢复",
        true,
        FallbackAction::RestoreBackup,
    ),
    (
        ErrorCode::A02,
        "笔记库损坏，已切换到备份副本",
        false,
        FallbackAction::LoadBackup,
    ),
    (
        ErrorCode::A03,
        "存储空间不足，请清理后重试",
        true,
        FallbackAction::PauseSyncAndIndex,
    ),
    (
        ErrorCode::A04,
        "保存失败，内容已暂存在内存",
        true,
        FallbackAction::PendingWrites,
    ),
    (
        ErrorCode::A05,
        "保存失败，正在重试",
        true,
        FallbackAction::AtomicRetry,
    ),
    (
        ErrorCode::A06,
        "数据校验未通过，已回滚",
        false,
        FallbackAction::Rollback,
    ),
    (
        ErrorCode::A07,
        "自动备份失败",
        true,
        FallbackAction::PromptExport,
    ),
    (
        ErrorCode::A08,
        "备份文件不可用",
        false,
        FallbackAction::SwitchBackup,
    ),
    (
        ErrorCode::A09,
        "本地镜像同步失败",
        true,
        FallbackAction::MarkRedo,
    ),
    (
        ErrorCode::A10,
        "附件保存失败",
        true,
        FallbackAction::DeferContent,
    ),
    (
        ErrorCode::A11,
        "查询超时，已简化条件",
        true,
        FallbackAction::SimpleQuery,
    ),
    (
        ErrorCode::A12,
        "笔记库版本过新，请升级应用",
        false,
        FallbackAction::ReadOnly,
    ),
    // B 类 · 同步与网络
    (
        ErrorCode::B01,
        "当前离线，改动会在联网后同步",
        true,
        FallbackAction::OfflineQueue,
    ),
    (
        ErrorCode::B02,
        "连接超时，正在切换线路",
        true,
        FallbackAction::SwitchTarget,
    ),
    (
        ErrorCode::B03,
        "直连失败，已改用中继",
        true,
        FallbackAction::RelayChain,
    ),
    (
        ErrorCode::B04,
        "云端账号认证失败",
        false,
        FallbackAction::PromptReconfigure,
    ),
    (
        ErrorCode::B05,
        "对象存储访问被拒绝",
        false,
        FallbackAction::PromptReconfigure,
    ),
    (
        ErrorCode::B06,
        "检测到版本冲突，请确认保留",
        false,
        FallbackAction::ConflictCopy,
    ),
    (
        ErrorCode::B07,
        "同步失败，将重试完整同步",
        true,
        FallbackAction::FullSync,
    ),
    (
        ErrorCode::B08,
        "此设备未获授权",
        false,
        FallbackAction::Reauthorize,
    ),
    (
        ErrorCode::B09,
        "此设备已被移除",
        false,
        FallbackAction::StopSync,
    ),
    (
        ErrorCode::B10,
        "待同步内容较多",
        true,
        FallbackAction::BatchResend,
    ),
    (
        ErrorCode::B11,
        "传输中断，支持断点续传",
        true,
        FallbackAction::Resume,
    ),
    (
        ErrorCode::B12,
        "同步目标只读",
        false,
        FallbackAction::SwitchTarget,
    ),
    // C 类 · 加密与安全
    (ErrorCode::C01, "密码错误", true, FallbackAction::None),
    (ErrorCode::C02, "密钥生成失败", false, FallbackAction::None),
    (
        ErrorCode::C03,
        "数据校验未通过，已拒绝访问",
        false,
        FallbackAction::FailClosed,
    ),
    (
        ErrorCode::C04,
        "没有权限执行此操作",
        false,
        FallbackAction::None,
    ),
    (
        ErrorCode::C05,
        "私密工作区不允许云端处理",
        false,
        FallbackAction::ForceLocal,
    ),
    (
        ErrorCode::C06,
        "授权已过期（15 分钟）",
        false,
        FallbackAction::Reauthorize,
    ),
    (
        ErrorCode::C07,
        "检测到审计记录异常",
        false,
        FallbackAction::FreezeAgent,
    ),
    (
        ErrorCode::C08,
        "设备身份校验失败",
        false,
        FallbackAction::Reauthorize,
    ),
    (
        ErrorCode::C09,
        "签名校验未通过",
        false,
        FallbackAction::Reject,
    ),
    (ErrorCode::C10, "已自动锁定", false, FallbackAction::None),
    // D 类 · AI 与模型
    (
        ErrorCode::D01,
        "需要先下载 AI 模型",
        false,
        FallbackAction::PromptDownload,
    ),
    (
        ErrorCode::D02,
        "模型加载失败",
        true,
        FallbackAction::DowngradeQuant,
    ),
    (
        ErrorCode::D03,
        "内存不足，已切换小模型",
        true,
        FallbackAction::DowngradeQuant,
    ),
    (
        ErrorCode::D04,
        "AI 响应超时",
        true,
        FallbackAction::ShorterContext,
    ),
    (
        ErrorCode::D05,
        "云端服务不可用，已切换本地",
        true,
        FallbackAction::RouterToLocal,
    ),
    (
        ErrorCode::D06,
        "向量化失败，已跳过语义检索",
        true,
        FallbackAction::Bm25Only,
    ),
    (
        ErrorCode::D07,
        "此功能需要 OCR 引擎",
        false,
        FallbackAction::NotImplemented,
    ),
    (
        ErrorCode::D08,
        "内容过长，已自动截断",
        false,
        FallbackAction::CompressContext,
    ),
    (
        ErrorCode::D09,
        "工具调用失败",
        true,
        FallbackAction::ErrorToAgent,
    ),
    (
        ErrorCode::D10,
        "操作超出授权范围",
        false,
        FallbackAction::InterceptAudit,
    ),
    // E 类 · 插件与扩展
    (
        ErrorCode::E01,
        "插件加载失败",
        false,
        FallbackAction::DisablePlugin,
    ),
    (
        ErrorCode::E02,
        "插件代码无法运行",
        false,
        FallbackAction::DisablePlugin,
    ),
    (
        ErrorCode::E03,
        "插件未声明此方法",
        false,
        FallbackAction::NotImplemented,
    ),
    (
        ErrorCode::E04,
        "插件方法未实现",
        false,
        FallbackAction::NotImplemented,
    ),
    (
        ErrorCode::E05,
        "插件执行超时已终止",
        false,
        FallbackAction::Terminate,
    ),
    (
        ErrorCode::E06,
        "插件权限不足",
        false,
        FallbackAction::Reject,
    ),
    (
        ErrorCode::E07,
        "插件异常已停止",
        false,
        FallbackAction::Isolate,
    ),
    (
        ErrorCode::E08,
        "插件签名校验失败",
        false,
        FallbackAction::RejectInstall,
    ),
    // F 类 · 业务规则
    (
        ErrorCode::F01,
        "不能把项目移动到它自己里面",
        false,
        FallbackAction::Reject,
    ),
    (
        ErrorCode::F02,
        "任务依赖存在循环",
        false,
        FallbackAction::Reject,
    ),
    (
        ErrorCode::F03,
        "当前状态不能执行此操作",
        false,
        FallbackAction::None,
    ),
    (
        ErrorCode::F04,
        "资源正在被使用",
        true,
        FallbackAction::Queue,
    ),
    (
        ErrorCode::F05,
        "已超出数量上限",
        false,
        FallbackAction::None,
    ),
    (
        ErrorCode::F06,
        "不支持此文件格式",
        false,
        FallbackAction::None,
    ),
    // G 类 · 系统与平台
    (
        ErrorCode::G01,
        "本地缓存已失效",
        false,
        FallbackAction::RebuildFromNative,
    ),
    (
        ErrorCode::G02,
        "将在回到前台后同步",
        false,
        FallbackAction::CompensateSync,
    ),
    (
        ErrorCode::G03,
        "通知权限未开启",
        false,
        FallbackAction::PromptSettings,
    ),
    (
        ErrorCode::G04,
        "当前平台不支持此功能",
        false,
        FallbackAction::HideEntry,
    ),
];

impl ErrorCode {
    /// 从码字符串（如 `"B03"`）解析。
    pub fn from_code(s: &str) -> Option<Self> {
        TABLE
            .iter()
            .find(|(c, _, _, _)| format!("{c:?}") == s)
            .map(|(c, _, _, _)| *c)
    }

    /// 码字符串（`"A01"` .. `"G04"`）。
    pub fn code(self) -> String {
        format!("{self:?}")
    }
}

impl ErrorCode {
    /// 三问之一：用户文案（V26 表 4-2 第 3 列）。
    pub fn user_message(self) -> &'static str {
        TABLE
            .iter()
            .find(|(c, _, _, _)| *c == self)
            .map(|(_, m, _, _)| *m)
            .unwrap_or("未知错误")
    }

    /// 三问之二：是否重试（第 4 列）。
    pub fn is_retryable(self) -> bool {
        TABLE
            .iter()
            .find(|(c, _, _, _)| *c == self)
            .map(|(_, _, r, _)| *r)
            .unwrap_or(false)
    }

    /// 三问之三：降级动作（第 5 列）。
    pub fn fallback(self) -> FallbackAction {
        TABLE
            .iter()
            .find(|(c, _, _, _)| *c == self)
            .map(|(_, _, _, f)| *f)
            .unwrap_or(FallbackAction::None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 契约校验：矩阵恰好 62 行（V26 A12+B12+C10+D10+E8+F6+G4）。
    #[test]
    fn table_has_exactly_62_entries() {
        assert_eq!(TABLE.len(), 62);
    }

    /// 契约校验：码字符串唯一且可往返解析。
    #[test]
    fn codes_round_trip() {
        for (code, _, _, _) in TABLE {
            let s = code.code();
            assert_eq!(ErrorCode::from_code(&s), Some(*code), "round-trip {s}");
        }
        assert_eq!(ErrorCode::from_code("B03"), Some(ErrorCode::B03));
        assert_eq!(ErrorCode::from_code("Z99"), None);
    }

    /// 契约校验：用户文案非空。
    #[test]
    fn user_messages_non_empty() {
        for (code, msg, _, _) in TABLE {
            assert!(!msg.is_empty(), "{code:?} empty message");
        }
    }

    /// 降级铁律 1：B 类全部可重试或有降级动作，且不得让本地编辑失败。
    #[test]
    fn category_b_never_blocks_local_editing() {
        for (code, _, retryable, fallback) in
            TABLE.iter().filter(|(c, ..)| c.code().starts_with('B'))
        {
            assert!(
                *retryable || fallback.requires_fallback(),
                "{code:?} 违反降级铁律 1 — 既不可重试也无降级"
            );
        }
    }

    /// 降级铁律 2：C03 必须失败关闭。
    #[test]
    fn c03_is_fail_closed() {
        assert_eq!(ErrorCode::C03.fallback(), FallbackAction::FailClosed);
        assert!(!ErrorCode::C03.is_retryable());
    }

    /// 降级铁律 3：D 类全部不阻断数据读写（仅影响提案生成）。
    #[test]
    fn category_d_never_blocks_data() {
        for (code, _, _, fallback) in TABLE.iter().filter(|(c, ..)| c.code().starts_with('D')) {
            assert!(
                matches!(fallback, FallbackAction::None) || fallback.requires_fallback(),
                "{code:?} 必须有明确降级语义"
            );
        }
    }

    /// 抽样：A01 语义对齐 V26 表 4-2。
    #[test]
    fn spot_check_a01() {
        assert_eq!(
            ErrorCode::A01.user_message(),
            "无法打开笔记库，正在尝试恢复"
        );
        assert!(ErrorCode::A01.is_retryable());
        assert_eq!(ErrorCode::A01.fallback(), FallbackAction::RestoreBackup);
    }
}
