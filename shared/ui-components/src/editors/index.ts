/**
 * 编辑器实体导出（DK-0F editor-uplift · 2026-09-25）
 *
 * 编辑器统一 ProseMirror + loro-prosemirror（ADR-005 D2）：
 * - auroraSchema：合并版 Schema（mobile 实战语义 + table/ai_suggestion 增量）
 * - createAuroraEditor：Loro-ProseMirror 编辑器装配（undo/redo + 快照链路）
 * - RichEditor：mobile 实战编辑器组件（真机验证 DK-05M-V GO）
 *
 * TipTap 系 DocumentEditor/CanvasEditor 已随 DK-0F 删除（双端 import=0）。
 */
export { auroraSchema, AURORA_BLOCK_TYPES, AURORA_BLOCK_CSS } from '../schema/auroraSchema';
export type { AuroraBlockType, TaskBlockAttrs, EmbedAttrs, AISuggestionAttrs } from '../schema/auroraSchema';
export {
  createAuroraEditor,
  loroDocFromBase64,
  bytesToBase64,
  base64ToBytes,
  extractPlainText,
  undo,
  redo,
  canUndo,
  canRedo,
} from './auroraEditor';
export type { AuroraEditorHandle, CreateAuroraEditorOptions } from './auroraEditor';
export { RichEditor } from './RichEditor';
export type { EditorPlatformBridge } from './RichEditor';
