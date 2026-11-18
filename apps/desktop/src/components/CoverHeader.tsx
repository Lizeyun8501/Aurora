/**
 * R4-1 → C-04 升级: Cover+图标 — 页面门面。
 *
 * - 封面: 内置渐变图集（本地 CSS 渐变——零外链 DoD④）+ 自定义图片
 *   （走附件与加密通道——onCustomCover 由宿主注入附件写链;命令未落地时
 *   按钮禁用+title 披露）。**无封面不渲染任何占位**（DoD③——v1 恒 72px
 *   色块的「每篇顶着无意义色块」问题修根）。
 * - 图标: VivoIcon（共享 NoteIcon 单一实现——aurora.icon.<id> 同字段,
 *   与移动端树/书签/搜索一致 DoD②）+ 选择器面板（16 def 网格）。
 * - 持久化: localStorage aurora.cover.<noteId>（{type:'grad',i}|
 *   {type:'img',attId}|null）——元数据级;附件通道落地后 img 档走
 *   cmd_read_attachment 解密读（DoD④ 零外链恒真）。
 * - 视觉克制: 封面 ≤160px;图标 20px 线性;次要素色面包屑不在本组件。
 */
import { useEffect, useRef, useState } from 'react';
import tokens from '../design/tokens';
import { VivoIcon, ALL_ICONS, getIconDef, setIconDef, type IconDef } from '@aurora/ui-components';

type Cover = { type: 'grad'; i: number } | { type: 'img'; attId: string } | null;

const COVER_KEY = (id: string) => `aurora.cover.${id}`;

/** 内置渐变图集（本地资源——CSS 渐变非图片,零外链;暗色系与 tokens 协调）。 */
const GRADS: Array<[string, string]> = [
  ['#1e3a5f', '#2d5a88'],
  ['#3b2f63', '#5c4a8f'],
  ['#1f4a3d', '#2f6b57'],
  ['#4a2f2f', '#6b4a3a'],
  ['#2f3b4a', '#4a5c6e'],
  ['#4a3b1f', '#6e5a2f'],
  ['#3d1f4a', '#5c2f6e'],
  ['#1f4a4a', '#2f6b6b'],
];

function loadCover(noteId: string): Cover {
  try {
    const raw = localStorage.getItem(COVER_KEY(noteId));
    return raw ? (JSON.parse(raw) as Cover) : null;
  } catch { return null; }
}
function saveCover(noteId: string, c: Cover): void {
  try {
    if (c === null) localStorage.removeItem(COVER_KEY(noteId));
    else localStorage.setItem(COVER_KEY(noteId), JSON.stringify(c));
  } catch { /* 防御 */ }
}

export default function CoverHeader({ noteId, onCustomCover }: {
  noteId: string;
  /** C-04: 自定义封面（附件与加密通道）——宿主注入:接收 File 返回附件 id;
   *  undefined=命令未落地（按钮禁用+title 披露——bravo-request-attach-file）。 */
  onCustomCover?: (file: File) => Promise<string | null>;
}): React.ReactElement {
  const [cover, setCover] = useState<Cover>(() => loadCover(noteId));
  const [icon, setIcon] = useState<IconDef>(() => getIconDef('doc', noteId));
  const [menu, setMenu] = useState(false);
  const [iconPick, setIconPick] = useState(false);
  const fileRef = useRef<HTMLInputElement | null>(null);
  useEffect(() => { setCover(loadCover(noteId)); setIcon(getIconDef('doc', noteId)); }, [noteId]);
  /* 总闸无障碍: Esc 关闭面板 + 外点关闭（键盘可达——两 picker 均非陷阱） */
  useEffect(() => {
    if (!menu && !iconPick) return;
    const onKey = (e: KeyboardEvent): void => { if (e.key === 'Escape') { setMenu(false); setIconPick(false); } };
    const onDown = (e: MouseEvent): void => {
      const t = e.target as HTMLElement;
      if (!t.closest('[data-testid="cover-header"]')) { setMenu(false); setIconPick(false); }
    };
    document.addEventListener('keydown', onKey);
    document.addEventListener('mousedown', onDown);
    return () => { document.removeEventListener('keydown', onKey); document.removeEventListener('mousedown', onDown); };
  }, [menu, iconPick]);

  const pickIcon = (def: IconDef): void => { setIconDef(noteId, def.id); setIcon(def); setIconPick(false); };
  const pickGrad = (i: number): void => { const c: Cover = { type: 'grad', i }; setCover(c); saveCover(noteId, c); setMenu(false); };
  const clearCover = (): void => { setCover(null); saveCover(noteId, null); setMenu(false); };

  const onFile = async (f: File | undefined): Promise<void> => {
    if (!f || !onCustomCover) return;
    const attId = await onCustomCover(f);
    if (attId) { const c: Cover = { type: 'img', attId }; setCover(c); saveCover(noteId, c); setMenu(false); }
  };

  /* 无封面零占位（DoD③）: cover===null 且菜单未开 → 只渲染图标钮一行,
     高度由图标钮自身（44px 常规操作件——非封面占位）;首屏正文位置差=0。 */
  const hasCover = cover !== null;
  return (
    <div data-testid="cover-header" style={{ marginBottom: tokens.spacing.sm, position: 'relative' }}>
      {hasCover ? (
        <div
          data-testid="cover-band"
          style={{
            height: 140, /* ≤160 上限——视觉克制档 */
            borderRadius: tokens.radius.md,
            background: cover?.type === 'grad'
              ? `linear-gradient(135deg, ${GRADS[cover.i % GRADS.length][0]}, ${GRADS[cover.i % GRADS.length][1]})`
              : 'rgba(255,255,255,0.06)',
            position: 'relative',
          }}
        >
          {/* 自定义图档: 附件 id 数据面（img src 由宿主经 cmd_read_attachment 注入——
              命令落地前此档不可达,渲染防御为中性底） */}
          {cover?.type === 'img' && (
            <img
              data-testid="cover-img"
              src={`attachment://${cover.attId}`}
              alt="封面"
              style={{ width: '100%', height: '100%', objectFit: 'cover', borderRadius: tokens.radius.md, display: 'none' }}
            />
          )}
        </div>
      ) : null}

      <div style={{ display: 'flex', alignItems: 'center', gap: 8, height: 44, marginTop: hasCover ? -22 : 0, paddingLeft: hasCover ? 14 : 2, position: 'relative' }}>
        <button
          onClick={() => setIconPick((v) => !v)}
          aria-label="选择页面图标"
          title="页面图标（与树/书签/搜索同源）"
          data-testid="cover-icon-btn"
          style={{ width: 44, height: 44, borderRadius: tokens.radius.md, background: tokens.color.bgElevated, border: '1px solid rgba(255,255,255,0.12)', display: 'flex', alignItems: 'center', justifyContent: 'center', cursor: 'pointer', padding: 0 }}
        >
          <VivoIcon def={icon} size={28} />
        </button>
        {iconPick && (
          <div role="menu" aria-label="图标选择" data-testid="icon-picker" style={{ position: 'absolute', top: 48, left: 0, zIndex: 40, background: tokens.color.bgElevated, border: '1px solid rgba(255,255,255,0.14)', borderRadius: 8, padding: 8, display: 'grid', gridTemplateColumns: 'repeat(8, 32px)', gap: 4, boxShadow: '0 8px 24px rgba(0,0,0,0.35)' }}>
            {ALL_ICONS.map((d) => (
              <button key={d.id} role="menuitem" onClick={() => pickIcon(d)} aria-label={d.label} title={d.label}
                style={{ width: 32, height: 32, border: 'none', borderRadius: 6, background: 'transparent', cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
                <VivoIcon def={d} size={22} />
              </button>
            ))}
          </div>
        )}
        <button
          onClick={() => setMenu((v) => !v)}
          aria-label="封面设置"
          data-testid="cover-menu-btn"
          style={{ border: '1px dashed rgba(255,255,255,0.2)', background: 'transparent', color: tokens.color.textSecondary, fontSize: 12, borderRadius: 6, padding: '4px 10px', cursor: 'pointer' }}
        >
          {hasCover ? '更换封面' : '添加封面'}
        </button>
        {menu && (
          <div role="menu" aria-label="封面图集" data-testid="cover-picker" style={{ position: 'absolute', top: 44, left: 0, zIndex: 40, background: tokens.color.bgElevated, border: '1px solid rgba(255,255,255,0.14)', borderRadius: 8, padding: 10, display: 'flex', gap: 8, boxShadow: '0 8px 24px rgba(0,0,0,0.35)' }}>
            {GRADS.map((g, i) => (
              <button key={i} role="menuitem" onClick={() => pickGrad(i)} aria-label={`渐变 ${i + 1}`}
                data-testid={`grad-${i}`}
                style={{ width: 56, height: 36, borderRadius: 6, border: 'none', cursor: 'pointer', background: `linear-gradient(135deg, ${g[0]}, ${g[1]})` }} />
            ))}
            <button
              role="menuitem"
              disabled={!onCustomCover}
              onClick={() => fileRef.current?.click()}
              aria-label="自定义封面图片"
              title={onCustomCover ? '选择图片（≤2MB,走附件加密通道）' : '自定义图片待附件写入命令（bravo-request-attach-file 已提）'}
              style={{ width: 56, height: 36, borderRadius: 6, border: '1px dashed rgba(255,255,255,0.24)', background: 'transparent', color: tokens.color.textSecondary, fontSize: 11, cursor: onCustomCover ? 'pointer' : 'not-allowed', opacity: onCustomCover ? 1 : 0.5 }}
            >＋图片</button>
            {hasCover && (
              <button role="menuitem" onClick={clearCover} aria-label="移除封面" data-testid="cover-clear"
                style={{ width: 56, height: 36, borderRadius: 6, border: 'none', background: 'rgba(248,113,113,0.14)', color: '#f87171', fontSize: 11, cursor: 'pointer' }}>移除</button>
            )}
            <input ref={fileRef} type="file" accept="image/*" hidden onChange={(e) => { void onFile(e.target.files?.[0]); e.target.value = ''; }} />
          </div>
        )}
      </div>
    </div>
  );
}
