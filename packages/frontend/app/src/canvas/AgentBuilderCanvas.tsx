import {
  ReactFlow,
  Background,
  Controls,
  Node,
  Edge,
  applyNodeChanges,
  applyEdgeChanges,
  NodeChange,
  EdgeChange,
  Connection,
  addEdge,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useCallback, useMemo, useState, useEffect } from "react";
import { Graph, RunState, StepState } from "./types";
import { ActionNode } from "./nodes/ActionNode";
import { InboundNode } from "./nodes/InboundNode";
import { FilterNode } from "./nodes/FilterNode";
import { OutboundNode } from "./nodes/OutboundNode";
import { PALETTE, PaletteItem } from "./palette";

const nodeTypes = {
  action: ActionNode,
  inbound: InboundNode,
  filter: FilterNode,
  outbound: OutboundNode,
};

export interface AgentBuilderCanvasProps {
  graph: Graph;
  mode: "edit" | "monitor";
  onEdit?: (graph: Graph) => void;
  runState?: RunState;
  onNodeClick?: (nodeId: string) => void;
}

export function AgentBuilderCanvas({
  graph,
  mode,
  onEdit,
  runState,
  onNodeClick,
}: AgentBuilderCanvasProps) {
  const [nodes, setNodes] = useState<Node[]>(() => toReactFlowNodes(graph, runState));
  const [edges, setEdges] = useState<Edge[]>(() => toReactFlowEdges(graph, runState));

  useEffect(() => {
    setNodes(toReactFlowNodes(graph, runState));
    setEdges(toReactFlowEdges(graph, runState));
  }, [graph, runState]);

  const onNodesChange = useCallback(
    (chs: NodeChange[]) => {
      if (mode !== "edit") return;
      setNodes((prev) => {
        const next = applyNodeChanges(chs, prev);
        onEdit?.(fromReactFlow(next, edges));
        return next;
      });
    },
    [mode, edges, onEdit],
  );

  const onEdgesChange = useCallback(
    (chs: EdgeChange[]) => {
      if (mode !== "edit") return;
      setEdges((prev) => {
        const next = applyEdgeChanges(chs, prev);
        onEdit?.(fromReactFlow(nodes, next));
        return next;
      });
    },
    [mode, nodes, onEdit],
  );

  const onConnect = useCallback(
    (conn: Connection) => {
      if (mode !== "edit") return;
      setEdges((prev) => {
        const next = addEdge({ ...conn, animated: true }, prev);
        onEdit?.(fromReactFlow(nodes, next));
        return next;
      });
    },
    [mode, nodes, onEdit],
  );

  const contextMenu = useMemo(
    () =>
      mode === "edit" ? (
        <PaletteSidebar
          onAdd={(item) => {
            const id = `n_${Date.now()}`;
            const node: Node = {
              id,
              type: item.category,
              position: { x: 80 + nodes.length * 40, y: 80 },
              data: { label: item.label, state: "idle" satisfies StepState },
            };
            setNodes((prev) => {
              const next = [...prev, node];
              onEdit?.(fromReactFlow(next, edges));
              return next;
            });
          }}
        />
      ) : null,
    [mode, nodes.length, edges, onEdit],
  );

  return (
    <div style={{ display: "flex", flex: 1, height: "100%" }}>
      {contextMenu}
      <div style={{ flex: 1 }}>
        <ReactFlow
          nodes={nodes}
          edges={edges}
          nodeTypes={nodeTypes}
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
          onConnect={onConnect}
          onNodeClick={(_, n) => onNodeClick?.(n.id)}
          fitView
        >
          <Background />
          <Controls />
        </ReactFlow>
      </div>
    </div>
  );
}

function PaletteSidebar({ onAdd }: { onAdd: (item: PaletteItem) => void }) {
  return (
    <div
      data-testid="palette"
      style={{ width: 220, borderRight: "1px solid var(--celerex-border)", padding: 12 }}
    >
      <h4 style={{ margin: "0 0 8px" }}>Palette</h4>
      {PALETTE.map((it) => (
        <button
          key={it.type}
          onClick={() => onAdd(it)}
          style={{
            display: "block",
            width: "100%",
            marginBottom: 6,
            padding: "6px 8px",
            borderRadius: 6,
            border: "1px solid var(--celerex-border)",
            background: "transparent",
            textAlign: "left",
            cursor: "pointer",
          }}
        >
          <span style={{ fontSize: 10, textTransform: "uppercase", opacity: 0.6 }}>
            {it.category}
          </span>
          <div>{it.label}</div>
        </button>
      ))}
    </div>
  );
}

function toReactFlowNodes(g: Graph, rs?: RunState): Node[] {
  return g.nodes.map((n, i) => ({
    id: n.id,
    type: n.type,
    position: n.position ?? { x: 80 + i * 180, y: 80 + (i % 3) * 100 },
    data: { label: (n.config["label"] as string) ?? n.id, state: rs?.steps[n.id] ?? "idle" },
  }));
}

function toReactFlowEdges(g: Graph, rs?: RunState): Edge[] {
  return g.edges.map((e) => ({
    id: `${e.from}->${e.to}`,
    source: e.from,
    target: e.to,
    animated: rs?.lastFlowedEdge?.from === e.from && rs?.lastFlowedEdge?.to === e.to,
  }));
}

function fromReactFlow(nodes: Node[], edges: Edge[]): Graph {
  return {
    nodes: nodes.map((n) => ({
      id: n.id,
      type: (n.type ?? "action") as Graph["nodes"][number]["type"],
      config: { label: (n.data as { label?: string })?.label ?? n.id },
      position: n.position,
    })),
    edges: edges
      .filter((e) => e.source && e.target)
      .map((e) => ({ from: e.source, to: e.target })),
  };
}
