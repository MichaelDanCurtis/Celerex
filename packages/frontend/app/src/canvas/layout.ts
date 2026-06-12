import { Graph } from "./types";

const COL_W = 270;
const ROW_H = 150;
const X0 = 80;
const Y0 = 90;

/**
 * Deterministic left-to-right DAG layout: each node's column is its longest
 * path from a root (node with no incoming edge), nodes in a column stack
 * vertically and are centered against the tallest column. Cycle-safe — on a
 * cycle the affected nodes just keep depth 0 rather than looping forever.
 */
export function autoLayout(graph: Graph): Graph {
  const ids = graph.nodes.map((n) => n.id);
  const incoming = new Map<string, string[]>(ids.map((id) => [id, []]));
  for (const e of graph.edges) {
    if (incoming.has(e.to) && ids.includes(e.from)) incoming.get(e.to)!.push(e.from);
  }

  // Longest-path depth via iterative relaxation (bounded by node count).
  const depth = new Map<string, number>(ids.map((id) => [id, 0]));
  for (let pass = 0; pass < ids.length; pass++) {
    let changed = false;
    for (const e of graph.edges) {
      const df = depth.get(e.from);
      const dt = depth.get(e.to);
      if (df === undefined || dt === undefined) continue;
      if (df + 1 > dt) {
        depth.set(e.to, df + 1);
        changed = true;
      }
    }
    if (!changed) break;
  }

  // Group nodes by column, preserving their existing order within a column.
  const columns = new Map<number, string[]>();
  for (const n of graph.nodes) {
    const d = depth.get(n.id) ?? 0;
    if (!columns.has(d)) columns.set(d, []);
    columns.get(d)!.push(n.id);
  }
  const tallest = Math.max(1, ...[...columns.values()].map((c) => c.length));

  const pos = new Map<string, { x: number; y: number }>();
  for (const [d, col] of columns) {
    // Center shorter columns against the tallest one.
    const offset = ((tallest - col.length) * ROW_H) / 2;
    col.forEach((id, i) => {
      pos.set(id, { x: X0 + d * COL_W, y: Y0 + offset + i * ROW_H });
    });
  }

  return {
    nodes: graph.nodes.map((n) => ({ ...n, position: pos.get(n.id) ?? n.position })),
    edges: graph.edges,
  };
}
