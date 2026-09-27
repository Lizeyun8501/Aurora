/**
 * DK-11 画布（域七）第一切片 — Canvas2D 无限画布骨架。
 *
 * 数据模型：节点 content_ref 引用（不内嵌正文），布局数据单独立文档。
 * 布局持久化第一切片走 WebView localStorage（key: aurora-canvas-v1）——
 * Rust 侧独立文档存储按重派书约束走 alpha-request 提案，不直改 crates/**。
 * 渲染：Canvas2D 起步（WebGL 置 Phase 5）；LOD/视口裁剪本切片先做视口裁剪。
 */
import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { tokens } from '../design/tokens';

export interface CanvasNode {
  id: string;
  /** 笔记内容引用（不内嵌正文）——空串表示未绑定笔记的便签 */
  content_ref: string;
  x: number;
  y: number;
  w: number;
  h: number;
  kind: 'note' | 'sticky';
  title: string;
}

export interface CanvasEdge {
  id: string;
  from: string;
  to: string;
  label?: string;
}

export interface CanvasViewport {
  x: number;
  y: number;
  scale: number;
}

export interface CanvasDoc {
  version: 1;
  nodes: CanvasNode[];
  edges: CanvasEdge[];
  viewport: CanvasViewport;
}

const LS_KEY = 'aurora-canvas-v1';

function loadDoc(): CanvasDoc {
  try {
    const raw = localStorage.getItem(LS_KEY);
    if (raw) {
      const doc = JSON.parse(raw) as CanvasDoc;
      if (doc && doc.version === 1 && Array.isArray(doc.nodes)) return doc;
    }
  } catch {
    /* 损坏文档视为空画布 */
  }
  return {
    version: 1,
    nodes: [
      { id: 'n-demo-1', content_ref: '', x: 120, y: 100, w: 180, h: 96, kind: 'sticky', title: '示例便签' },
      { id: 'n-demo-2', content_ref: '', x: 420, y: 240, w: 180, h: 96, kind: 'sticky', title: '双击空白新建' },
    ],
    edges: [{ id: 'e-demo-1', from: 'n-demo-1', to: 'n-demo-2' }],
    viewport: { x: 0, y: 0, scale: 1 },
  };
}

function saveDoc(doc: CanvasDoc): void {
  try {
    localStorage.setItem(LS_KEY, JSON.stringify(doc));
  } catch {
    /* 存储满/禁用时静默（第一切片） */
  }
}

function uid(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.floor(Math.random() * 1e4).toString(36)}`;
}

/** 世界坐标 → 屏幕坐标 */
function w2s(v: CanvasViewport, wx: number, wy: number): [number, number] {
  return [wx * v.scale + v.x, wy * v.scale + v.y];
}
/** 屏幕坐标 → 世界坐标 */
function s2w(v: CanvasViewport, sx: number, sy: number): [number, number] {
  return [(sx - v.x) / v.scale, (sy - v.y) / v.scale];
}

export default function CanvasView(): React.ReactElement {
  const [doc, setDoc] = useState<CanvasDoc>(() => loadDoc());
  const [selected, setSelected] = useState<string | null>(null);
  const [fps, setFps] = useState<number | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const docRef = useRef(doc);
  docRef.current = doc;
  const dragRef = useRef<
    | { kind: 'pan'; sx: number; sy: number; vx: number; vy: number }
    | { kind: 'node'; id: string; dx: number; dy: number }
    | null
  >(null);

  const persist = useCallback((next: CanvasDoc) => {
    setDoc(next);
    saveDoc(next);
  }, []);

  /** 视口裁剪：只绘制视口内（含边距）的节点 */
  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    const wrap = wrapRef.current;
    if (!canvas || !wrap) return;
    const dpr = window.devicePixelRatio || 1;
    const cw = wrap.clientWidth;
    const ch = wrap.clientHeight;
    if (canvas.width !== cw * dpr || canvas.height !== ch * dpr) {
      canvas.width = cw * dpr;
      canvas.height = ch * dpr;
      canvas.style.width = `${cw}px`;
      canvas.style.height = `${ch}px`;
    }
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, cw, ch);
    const v = docRef.current.viewport;

    // 网格背景（随视口）
    const grid = 40 * v.scale;
    if (grid > 8) {
      ctx.strokeStyle = 'rgba(255,255,255,0.05)';
      ctx.lineWidth = 1;
      ctx.beginPath();
      const ox = v.x % grid;
      const oy = v.y % grid;
      for (let x = ox; x < cw; x += grid) {
        ctx.moveTo(x, 0);
        ctx.lineTo(x, ch);
      }
      for (let y = oy; y < ch; y += grid) {
        ctx.moveTo(0, y);
        ctx.lineTo(cw, y);
      }
      ctx.stroke();
    }

    // 连线
    ctx.strokeStyle = tokens.color.textSecondary;
    ctx.lineWidth = 1.5;
    for (const e of docRef.current.edges) {
      const a = docRef.current.nodes.find((n) => n.id === e.from);
      const b = docRef.current.nodes.find((n) => n.id === e.to);
      if (!a || !b) continue;
      const [ax, ay] = w2s(v, a.x + a.w / 2, a.y + a.h / 2);
      const [bx, by] = w2s(v, b.x + b.w / 2, b.y + b.h / 2);
      ctx.beginPath();
      ctx.moveTo(ax, ay);
      ctx.bezierCurveTo((ax + bx) / 2, ay, (ax + bx) / 2, by, bx, by);
      ctx.stroke();
      if (e.label) {
        ctx.fillStyle = tokens.color.textSecondary;
        ctx.font = `${12}px sans-serif`;
        ctx.fillText(e.label, (ax + bx) / 2, (ay + by) / 2 - 6);
      }
    }

    // 节点（视口裁剪）
    for (const n of docRef.current.nodes) {
      const [sx, sy] = w2s(v, n.x, n.y);
      const sw = n.w * v.scale;
      const sh = n.h * v.scale;
      if (sx + sw < -40 || sy + sh < -40 || sx > cw + 40 || sy > ch + 40) continue;
      const r = 8 * v.scale;
      ctx.fillStyle = n.kind === 'note' ? tokens.color.bgElevated : 'rgba(255,214,102,0.16)';
      ctx.strokeStyle = selected === n.id ? tokens.color.primaryBright : 'rgba(255,255,255,0.22)';
      ctx.lineWidth = selected === n.id ? 2.5 : 1.2;
      ctx.beginPath();
      ctx.roundRect(sx, sy, sw, sh, r);
      ctx.fill();
      ctx.stroke();
      if (v.scale > 0.4) {
        ctx.fillStyle = tokens.color.textPrimary;
        ctx.font = `${Math.max(11, 13 * v.scale)}px sans-serif`;
        ctx.fillText(n.title, sx + 10 * v.scale, sy + 22 * v.scale, sw - 16 * v.scale);
        if (n.content_ref) {
          ctx.fillStyle = tokens.color.textSecondary;
          ctx.font = `${Math.max(9, 11 * v.scale)}px sans-serif`;
          ctx.fillText(`↗ ${n.content_ref}`, sx + 10 * v.scale, sy + sh - 10 * v.scale, sw - 16 * v.scale);
        }
      }
    }
  }, [selected]);

  useEffect(() => {
    draw();
  }, [draw, doc]);

  /** 容器尺寸变化重绘 */
  useEffect(() => {
    const wrap = wrapRef.current;
    if (!wrap || typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(() => draw());
    ro.observe(wrap);
    return () => ro.disconnect();
  }, [draw]);

  const hitNode = useCallback((sx: number, sy: number): CanvasNode | null => {
    const v = docRef.current.viewport;
    const [wx, wy] = s2w(v, sx, sy);
    for (let i = docRef.current.nodes.length - 1; i >= 0; i--) {
      const n = docRef.current.nodes[i];
      if (wx >= n.x && wx <= n.x + n.w && wy >= n.y && wy <= n.y + n.h) return n;
    }
    return null;
  }, []);

  const onPointerDown = useCallback(
    (e: React.PointerEvent<HTMLCanvasElement>) => {
      const rect = (e.currentTarget as HTMLCanvasElement).getBoundingClientRect();
      const sx = e.clientX - rect.left;
      const sy = e.clientY - rect.top;
      const hit = hitNode(sx, sy);
      const v = docRef.current.viewport;
      if (hit) {
        setSelected(hit.id);
        const [wx, wy] = s2w(v, sx, sy);
        dragRef.current = { kind: 'node', id: hit.id, dx: wx - hit.x, dy: wy - hit.y };
      } else {
        setSelected(null);
        dragRef.current = { kind: 'pan', sx, sy, vx: v.x, vy: v.y };
      }
      e.currentTarget.setPointerCapture(e.pointerId);
    },
    [hitNode],
  );

  /** DK-11：原生 dblclick 绑定——pointer capture 下 React 合成 dblclick 不可靠（A2 实证） */
  useEffect(() => {
    const cv = canvasRef.current;
    if (!cv) return;
    const h = (e: MouseEvent) => { onDoubleClickRef.current(e); };
    cv.addEventListener('dblclick', h);
    return () => cv.removeEventListener('dblclick', h);
  }, []);

  const onPointerMove = useCallback(
    (e: React.PointerEvent<HTMLCanvasElement>) => {
      const drag = dragRef.current;
      if (!drag) return;
      const rect = (e.currentTarget as HTMLCanvasElement).getBoundingClientRect();
      const sx = e.clientX - rect.left;
      const sy = e.clientY - rect.top;
      if (drag.kind === 'pan') {
        setDoc((d) => ({ ...d, viewport: { ...d.viewport, x: drag.vx + (sx - drag.sx), y: drag.vy + (sy - drag.sy) } }));
      } else {
        const v = docRef.current.viewport;
        const [wx, wy] = s2w(v, sx, sy);
        setDoc((d) => ({
          ...d,
          nodes: d.nodes.map((n) => (n.id === drag.id ? { ...n, x: wx - drag.dx, y: wy - drag.dy } : n)),
        }));
      }
    },
    [],
  );

  const onPointerUp = useCallback(() => {
    if (dragRef.current) {
      dragRef.current = null;
      saveDoc(docRef.current);
    }
  }, []);

  /** wheel 缩放：以鼠标为中心 */
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const rect = canvas.getBoundingClientRect();
      const sx = e.clientX - rect.left;
      const sy = e.clientY - rect.top;
      setDoc((d) => {
        const v = d.viewport;
        const factor = e.deltaY < 0 ? 1.1 : 1 / 1.1;
        const scale = Math.min(4, Math.max(0.15, v.scale * factor));
        const [wx, wy] = s2w(v, sx, sy);
        const nx = sx - wx * scale;
        const ny = sy - wy * scale;
        const next = { ...d, viewport: { x: nx, y: ny, scale } };
        saveDoc(next);
        return next;
      });
    };
    canvas.addEventListener('wheel', onWheel, { passive: false });
    return () => canvas.removeEventListener('wheel', onWheel);
  }, []);

  const onDoubleClickRef = useRef<(e: MouseEvent) => void>(() => {});
  /** 双击空白：新建便签节点（以视口中心为锚） */
  const onDoubleClick = useCallback(
    (e: MouseEvent) => {
      const rect = (e.currentTarget as HTMLCanvasElement).getBoundingClientRect();
      const v = docRef.current.viewport;
      const [wx, wy] = s2w(v, e.clientX - rect.left, e.clientY - rect.top);
      const node: CanvasNode = {
        id: uid('n'),
        content_ref: '',
        x: wx - 90,
        y: wy - 48,
        w: 180,
        h: 96,
        kind: 'sticky',
        title: '新便签',
      };
      persist({ ...docRef.current, nodes: [...docRef.current.nodes, node] });
      setSelected(node.id);
    },
    [persist],
  );

  const deleteSelected = useCallback(() => {
    if (!selected) return;
    persist({
      ...docRef.current,
      nodes: docRef.current.nodes.filter((n) => n.id !== selected),
      edges: docRef.current.edges.filter((e) => e.from !== selected && e.to !== selected),
    });
    setSelected(null);
  }, [selected, persist]);
  onDoubleClickRef.current = onDoubleClick;

  const applyGridLayout = useCallback(() => {
    const nodes = docRef.current.nodes;
    const cols = Math.ceil(Math.sqrt(Math.max(1, nodes.length)));
    const gap = 40;
    const w = 180;
    const h = 96;
    const next = nodes.map((n, i) => ({ ...n, x: 60 + (i % cols) * (w + gap), y: 60 + Math.floor(i / cols) * (h + gap) }));
    persist({ ...docRef.current, nodes: next });
  }, [persist]);

  const exportJson = useCallback(() => {
    const blob = new Blob([JSON.stringify(docRef.current, null, 2)], { type: 'application/json' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = 'canvas.json';
    a.click();
    URL.revokeObjectURL(a.href);
  }, []);

  /** 1000 节点压测注入（验证脚本用；UI 面隐藏入口 aria-hidden） */
  const stressNodes = useMemo(
    () =>
      Array.from({ length: 1000 }, (_, i) => ({
        id: `stress-${i}`,
        content_ref: '',
        x: (i % 40) * 230 + 40,
        y: Math.floor(i / 40) * 140 + 40,
        w: 180,
        h: 96,
        kind: 'sticky' as const,
        title: `节点 ${i + 1}`,
      })),
    [],
  );
  const runFpsProbe = useCallback(() => {
    const merged = [...docRef.current.nodes.filter((n) => !n.id.startsWith('stress-')), ...stressNodes];
    persist({ ...docRef.current, nodes: merged });
    let frames = 0;
    const samples: number[] = [];
    let last = performance.now();
    let raf = 0;
    const tick = (t: number) => {
      frames++;
      const dt = t - last;
      last = t;
      if (dt > 0) samples.push(dt);
      if (t - (tick as unknown as { t0?: number }).t0! < 2000) {
        raf = requestAnimationFrame(tick);
      } else {
        samples.sort((a, b) => a - b);
        const p95 = samples[Math.floor(samples.length * 0.95)] ?? 0;
        setFps(Math.round(1000 / Math.max(p95, 1)));
        cancelAnimationFrame(raf);
      }
    };
    (tick as unknown as { t0?: number }).t0 = performance.now();
    raf = requestAnimationFrame(tick);
  }, [persist, stressNodes]);

  const btn = {
    background: tokens.color.bgElevated,
    color: tokens.color.textPrimary,
    border: '1px solid rgba(255,255,255,0.12)',
    borderRadius: tokens.radius.md,
    padding: `${tokens.spacing.xs + 2}px ${tokens.spacing.sm + 2}px`,
    fontSize: tokens.typography.caption.size,
    cursor: 'pointer',
    minHeight: 32,
  } as const;

  return (
    <main style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }} aria-label="画布视图">
      <div style={{ display: 'flex', gap: tokens.spacing.xs, padding: tokens.spacing.sm, alignItems: 'center' }}>
        <button style={btn} onClick={() => persist({ ...docRef.current, viewport: { x: 0, y: 0, scale: 1 } })}>
          重置视图
        </button>
        <button style={btn} onClick={applyGridLayout}>
          网格布局
        </button>
        <button style={btn} onClick={exportJson}>
          导出 JSON
        </button>
        <button style={btn} onClick={deleteSelected} disabled={!selected}>
          删除选中
        </button>
        <button style={btn} onClick={runFpsProbe} data-testid="fps-probe">
          1000 节点压测
        </button>
        <span style={{ fontSize: tokens.typography.caption.size, color: tokens.color.textSecondary }} data-testid="canvas-stats">
          {doc.nodes.length} 节点 · {doc.edges.length} 连线 · 视口 {Math.round(doc.viewport.x)},{Math.round(doc.viewport.y)}
          {doc.viewport.scale !== 1 && ` · ${Math.round(doc.viewport.scale * 100)}%`}
          {fps !== null && ` · P95 ${fps}fps`}
        </span>
      </div>
      <div ref={wrapRef} style={{ flex: 1, minHeight: 0, position: 'relative' }}>
        <canvas
          ref={canvasRef}
          role="img"
          aria-label="无限画布。拖拽平移，滚轮缩放，双击空白新建便签。"
          data-testid="canvas-surface"
          style={{ position: 'absolute', inset: 0, cursor: 'grab', touchAction: 'none' }}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
        />
      </div>
    </main>
  );
}
