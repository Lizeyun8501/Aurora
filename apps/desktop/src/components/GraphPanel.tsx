/**
 * DK-链接图谱原型（2.1 尾巴之二）— 全库 wiki 链接图谱视图。
 *
 * - 数据：cmd_links_graph() → {nodes, edges, truncated}（锁定篇参与
 *   title 空 → 显示短 id；悬挂目标虚节点虚线边；80 节点 degree 裁剪）；
 * - 布局：静态预计算力导向（spring-embedder 简版：斥力+弹簧引力 120
 *   迭代，圆周种子——无动画，v1 诚实边界）；
 * - 呈现纪律：SVG text 原样渲染（不可信内容零 HTML interpret）；
 * - 交互 v1：节点点击跳转（onSelect 路由链）；hover 高亮邻接边+邻居；
 *   缩放/拖拽延后；invoke=null（browser-mock）或失败 → 空态降级。
 */
import React, { useMemo, useState, useEffect } from 'react';

type InvokeFn = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

type GraphNode = { id: string | null; title: string; degree: number };
type GraphEdge = { srcId: string; dstTitle: string };
type LinksGraph = { nodes: GraphNode[]; edges: GraphEdge[]; truncated: boolean };

/** 静态力导向布局：返回每节点 [x, y]。斥力（库仑）+边弹簧（胡克）。 */
export function layoutGraph(
  nodes: GraphNode[],
  edges: GraphEdge[],
  w: number,
  h: number,
): [number, number][] {
  const n = nodes.length;
  const pos: [number, number][] = [];
  if (n === 0) return pos;
  // 圆周种子（确定性——同数据同布局）
  const R = Math.min(w, h) / 3;
  for (let i = 0; i < n; i++) {
    const a = (2 * Math.PI * i) / n;
    pos.push([w / 2 + R * Math.cos(a), h / 2 + R * Math.sin(a)]);
  }
  // 节点索引辅助：id→idx / title→idx（虚节点按 title 键）
  const idxOf = new Map<string, number>();
  nodes.forEach((nd, i) => {
    if (nd.id !== null) idxOf.set(`id:${nd.id}`, i);
    idxOf.set(`t:${nd.title}`, i);
  });
  // 边 → 节点对索引（src 实节点必有 id；dst 按 title 键）
  const pairs: [number, number][] = [];
  for (const e of edges) {
    const s = idxOf.get(`id:${e.srcId}`);
    const d = idxOf.get(`t:${e.dstTitle}`);
    if (s !== undefined && d !== undefined && s !== d) pairs.push([s, d]);
  }
  // 120 迭代：斥力全部对 + 弹簧边
  const ITER = 120;
  const REPULSE = 2200;
  const SPRING_K = 90;
  for (let it = 0; it < ITER; it++) {
    const fx = new Array(n).fill(0);
    const fy = new Array(n).fill(0);
    for (let i = 0; i < n; i++) {
      for (let j = i + 1; j < n; j++) {
        let dx = pos[i][0] - pos[j][0];
        let dy = pos[i][1] - pos[j][1];
        let d2 = dx * dx + dy * dy;
        if (d2 < 1) {
          dx = (i - j) * 0.5 + 0.1;
          dy = (j - i) * 0.5 + 0.1;
          d2 = dx * dx + dy * dy;
        }
        const d = Math.sqrt(d2);
        const f = REPULSE / d2;
        fx[i] += (dx / d) * f;
        fy[i] += (dy / d) * f;
        fx[j] -= (dx / d) * f;
        fy[j] -= (dy / d) * f;
      }
    }
    for (const [s, d] of pairs) {
      const dx = pos[d][0] - pos[s][0];
      const dy = pos[d][1] - pos[s][1];
      const dist = Math.sqrt(dx * dx + dy * dy) || 0.01;
      const f = (dist - SPRING_K) * 0.02;
      fx[s] += (dx / dist) * f;
      fy[s] += (dy / dist) * f;
      fx[d] -= (dx / dist) * f;
      fy[d] -= (dy / dist) * f;
    }
    // 位移限幅（温度衰减——收敛稳定）
    const temp = 8 * (1 - it / ITER) + 0.5;
    for (let i = 0; i < n; i++) {
      const d = Math.sqrt(fx[i] * fx[i] + fy[i] * fy[i]) || 0.001;
      const step = Math.min(d, temp);
      pos[i][0] = Math.max(20, Math.min(w - 20, pos[i][0] + (fx[i] / d) * step));
      pos[i][1] = Math.max(20, Math.min(h - 20, pos[i][1] + (fy[i] / d) * step));
    }
  }
  return pos;
}

export default function GraphPanel(props: {
  invoke: InvokeFn | null;
  onSelect: (id: string) => void;
}): React.ReactElement {
  const { invoke, onSelect } = props;
  const [graph, setGraph] = useState<LinksGraph | null>(null);
  const [failed, setFailed] = useState(false);
  const [hover, setHover] = useState<number | null>(null);

  useEffect(() => {
    let alive = true;
    if (!invoke) {
      setGraph({ nodes: [], edges: [], truncated: false });
      return () => {
        alive = false;
      };
    }
    invoke('cmd_links_graph', {})
      .then((g) => {
        if (alive) setGraph(g as LinksGraph);
      })
      .catch(() => {
        if (alive) setFailed(true);
      });
    return () => {
      alive = false;
    };
  }, [invoke]);

  const W = 560;
  const H = 320;
  const pos = useMemo(
    () => (graph ? layoutGraph(graph.nodes, graph.edges, W, H) : []),
    [graph],
  );

  if (failed) {
    return (
      <section style={S.box}>
        <h4 style={S.h4}>链接图谱 · 加载失败（已降级）</h4>
      </section>
    );
  }
  if (!graph) {
    return (
      <section style={S.box}>
        <h4 style={S.h4}>链接图谱 · …</h4>
      </section>
    );
  }
  if (graph.nodes.length === 0) {
    return (
      <section style={S.box}>
        <h4 style={S.h4}>链接图谱</h4>
        <p style={S.empty}>暂无链接关系（在笔记中写 [[标题]] 建立链接）</p>
      </section>
    );
  }

  // hover 邻接计算：节点 idx → 相邻边 idx 集
  const idxOf = new Map<string, number>();
  graph.nodes.forEach((nd, i) => {
    if (nd.id !== null) idxOf.set(`id:${nd.id}`, i);
    idxOf.set(`t:${nd.title}`, i);
  });
  const edgePairs = graph.edges.map((e) => ({
    e,
    s: idxOf.get(`id:${e.srcId}`),
    d: idxOf.get(`t:${e.dstTitle}`),
  }));
  const neighborEdges =
    hover === null
      ? new Set<number>()
      : new Set(
          edgePairs
            .map((p, i) => (p.s === hover || p.d === hover ? i : -1))
            .filter((i) => i >= 0),
        );
  const neighborNodes =
    hover === null
      ? new Set<number>()
      : new Set(
          [...neighborEdges].flatMap((i) => {
            const p = edgePairs[i];
            return [p.s, p.d].filter((x): x is number => x !== undefined);
          }),
        );

  return (
    <section style={S.box}>
      <h4 style={S.h4}>
        链接图谱 · {graph.nodes.length} 节点 · {graph.edges.length} 边
        {graph.truncated ? ' · 已裁剪（仅显示连接最多的 80 篇）' : ''}
        <span style={S.meta}>（静态布局 · 点击节点跳转）</span>
      </h4>
      <svg width={W} height={H} style={S.svg} role="img" aria-label="全库链接图谱">
        {edgePairs.map(({ s, d }, i) => {
          if (s === undefined || d === undefined) return null;
          const [x1, y1] = pos[s];
          const [x2, y2] = pos[d];
          const dstVirtual = graph.nodes[d].id === null;
          const hl = neighborEdges.has(i);
          return (
            <line
              key={`e${i}`}
              x1={x1}
              y1={y1}
              x2={x2}
              y2={y2}
              stroke={hl ? '#2563eb' : '#cbd5e1'}
              strokeWidth={hl ? 1.6 : 1}
              strokeDasharray={dstVirtual ? '4 3' : undefined}
            />
          );
        })}
        {graph.nodes.map((nd, i) => {
          const [x, y] = pos[i];
          const r = 4 + Math.min(nd.degree, 8) * 1.2;
          const label = nd.id === null ? `${nd.title}（虚）` : nd.title || nd.id.slice(0, 8);
          const dim = hover !== null && !neighborNodes.has(i);
          return (
            <g
              key={`n${i}`}
              opacity={dim ? 0.3 : 1}
              style={{ cursor: nd.id ? 'pointer' : 'default' }}
              onMouseEnter={() => setHover(i)}
              onMouseLeave={() => setHover(null)}
              onClick={() => nd.id && onSelect(nd.id)}
            >
              <circle
                cx={x}
                cy={y}
                r={r}
                fill={nd.id === null ? '#f1f5f9' : '#93c5fd'}
                stroke={nd.id === null ? '#cbd5e1' : '#2563eb'}
              />
              <text x={x + r + 3} y={y + 3} fontSize={10} fill="#334155">
                {label}
              </text>
            </g>
          );
        })}
      </svg>
    </section>
  );
}

const S: Record<string, React.CSSProperties> = {
  box: {
    margin: '12px 0',
    padding: '10px 12px',
    border: '1px solid #e2e8f0',
    borderRadius: 8,
    background: '#fafbfc',
  },
  h4: { margin: '0 0 8px', fontSize: 13, color: '#475569' },
  meta: { marginLeft: 8, fontWeight: 400, fontSize: 11, color: '#94a3b8' },
  empty: { margin: 0, fontSize: 12, color: '#94a3b8' },
  svg: { display: 'block', border: '1px solid #eef2f7', borderRadius: 6, background: '#fff' },
};
