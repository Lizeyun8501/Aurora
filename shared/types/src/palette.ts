/**
 * C-12: 命令面板最近使用——TS 镜像（Rust 权威: crates/aurora-core/src/
 * palette_recents.rs;include_str 契约测试互证——字段漂移即红）。
 *
 * 双端单源 KV 键（core 直存 JSON;桌面 localStorage 旧源双写过渡——
 * C-12=双写段,切读下卡）。
 */

/** KV 单键（与 Rust RECENT_COMMANDS_KEY 常量一致——契约测试锚）。 */
export const RECENT_COMMANDS_KEY = 'palette:recent_commands';

/** LRU 上限（与 Rust MAX_RECENT_COMMANDS 一致——双端截断口径统一）。 */
export const RECENT_COMMANDS_MAX = 50;

/** 最近使用条目（serde 蛇形直通——字段 id/ts/count）。 */
export interface RecentCommand {
  /** 命令/笔记面板项 id（如 note.open / recent:<id>）。 */
  id: string;
  /** 最近一次执行时刻（epoch 毫秒——Date.now() 对齐）。 */
  ts: number;
  /** 累计执行次数（同 id track 合并;首记 =1）。 */
  count: number;
}
