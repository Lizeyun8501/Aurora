/**
 * @aurora/shared-types
 *
 * TypeScript definitions mirroring the Aurora Note Rust core domain
 * (`crates/aurora-core/`). A-08 起含日历运行时纯函数（calendar.ts——
 * repeatUtil/lunar,双端单份实现,禁止各自复制）。
 */

export * from './blocks';
export * from './knowledge';
export * from './gtd';
export * from './ai';
export * from './assets';
export * from './settings';
export * from './today';
export * from './sync';
export * from './security';
export * from './plugin';
export * from './events';
export * from './adapter';
export * from './calendar'; // A-08: 日历 shared 面（repeatUtil 上移+lunar+CalendarVm）
