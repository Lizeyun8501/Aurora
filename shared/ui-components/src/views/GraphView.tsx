import { useMemo, type ReactElement } from 'react';
import type { KnowledgeGraph } from '@aurora/shared-types';
import clsx from 'clsx';

export interface GraphViewProps {
  graph: KnowledgeGraph;
  className?: string;
}

const VIEW_WIDTH = 600;
const VIEW_HEIGHT = 600;
const CENTER_X = VIEW_WIDTH / 2;
const CENTER_Y = VIEW_HEIGHT / 2;
const RADIUS = 240;

/**
 * Renders a KnowledgeGraph as an SVG.
 *
 * SIMPLIFIED: nodes are laid out on a circle (no force simulation). For graphs
 * larger than ~1000 nodes a D3 force or WebGL renderer should replace this to
 * maintain interactive frame-rates.
 */
export function GraphView({ graph, className }: GraphViewProps): ReactElement {
  const positions = useMemo(() => {
    const map = new Map<string, { x: number; y: number }>();
    const count = graph.nodes.length;
    graph.nodes.forEach((node, i) => {
      const angle = (2 * Math.PI * i) / Math.max(count, 1);
      map.set(node.id, {
        x: CENTER_X + RADIUS * Math.cos(angle),
        y: CENTER_Y + RADIUS * Math.sin(angle),
      });
    });
    return map;
  }, [graph.nodes]);

  return (
    <section className={clsx('aurora-graph-view', className)} aria-label="Knowledge graph">
      <svg
        className="aurora-graph-svg"
        viewBox={`0 0 ${VIEW_WIDTH} ${VIEW_HEIGHT}`}
        role="img"
        aria-label="Knowledge graph visualization"
      >
        {graph.edges.map((edge) => {
          const a = positions.get(edge.source);
          const b = positions.get(edge.target);
          if (!a || !b) return null;
          return (
            <line
              key={edge.id}
              x1={a.x}
              y1={a.y}
              x2={b.x}
              y2={b.y}
              className="aurora-graph-edge"
              stroke="currentColor"
              strokeWidth={Math.max(1, Math.min(edge.weight, 4))}
            />
          );
        })}
        {graph.nodes.map((node) => {
          const pos = positions.get(node.id);
          if (!pos) return null;
          return (
            <g key={node.id} className="aurora-graph-node">
              <title>{node.title}</title>
              <circle cx={pos.x} cy={pos.y} r={6} className="aurora-graph-node-circle" />
              <text x={pos.x + 10} y={pos.y + 4} className="aurora-graph-node-label">
                {node.title}
              </text>
            </g>
          );
        })}
      </svg>
    </section>
  );
}
