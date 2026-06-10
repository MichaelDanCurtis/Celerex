import {
  ReactFlow,
  ReactFlowProvider,
  Background,
  BackgroundVariant,
  Controls,
  MiniMap,
  Node,
  Edge,
  NodeChange,
  EdgeChange,
  Connection,
  MarkerType,
  applyNodeChanges,
  applyEdgeChanges,
  addEdge,
  useReactFlow,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useCallback, useRef, useState, useEffect } from "react";
import { Graph, RunState, StepState, NodeCategory, FlowNodeData } from "./types";
import { FlowNode } from "./nodes/FlowNode";
import {
  PALETTE_GROUPS,
  PaletteItem,
  CATEGORY_META,
  paletteItemByType,
  accentFor,
} from "./palette";

const nodeTypes = {
  inbound: FlowNode,
  action: FlowNode,
  filter: FlowNode,
  outbound: FlowNode,
};

const DRAG_KEY = "application/celerex-node";

export interface AgentBuilderCanvasProps {
  graph: Graph;
  mode: "edit" | "monitor";
  onEdit?: (graph: Graph) => void;
  runState?: RunState;
  onNodeClick?: (nodeId: string) => void;
}

export function AgentBuilderCanvas(props: AgentBuilderCanvasProps) {
  return (
    <ReactFlowProvider>
      <CanvasInner {...props} />
    </ReactFlowProvider>
  );
}

function CanvasInner({ graph, mode, onEdit, runState, onNodeClick }: AgentBuilderCanvasProps) {
  const { screenToFlowPosition } = useReactFlow();
  const wrapperRef = useRef<HTMLDivElement>(null);
  const addCounter = useRef(0);
  const [nodes, setNodes] = useState<Node[]>(() => toReactFlowNodes(graph, runState));
  const [edges, setEdges] = useState<Edge[]>(() => toReactFlowEdges(graph, runState));

  // Refs mirror the latest nodes/edges so change handlers never echo a STALE
  // sibling collection back to the parent graph. (The classic bug: ReactFlow's
  // on-mount dimension-measurement fires onNodesChange while the `edges` closure
  // is still the empty initial array, wiping every edge.)
  const nodesRef = useRef<Node[]>(nodes);
  const edgesRef = useRef<Edge[]>(edges);
  nodesRef.current = nodes;
  edgesRef.current = edges;

  useEffect(() => {
    setNodes(toReactFlowNodes(graph, runState));
    setEdges(toReactFlowEdges(graph, runState));
  }, [graph, runState]);

  // Only structural edits (move / add / remove) should rewrite the parent graph.
  // Echoing dimension/select changes would re-sync the graph and loop.
  const onNodesChange = useCallback(
    (chs: NodeChange[]) => {
      if (mode !== "edit") return;
      const structural = chs.some((c) =>
        c.type === "position" || c.type === "remove" || c.type === "add" || c.type === "replace",
      );
      setNodes((prev) => {
        const next = applyNodeChanges(chs, prev);
        if (structural) onEdit?.(fromReactFlow(next, edgesRef.current));
        return next;
      });
    },
    [mode, onEdit],
  );

  const onEdgesChange = useCallback(
    (chs: EdgeChange[]) => {
      if (mode !== "edit") return;
      const structural = chs.some(
        (c) => c.type === "remove" || c.type === "add" || c.type === "replace",
      );
      setEdges((prev) => {
        const next = applyEdgeChanges(chs, prev);
        if (structural) onEdit?.(fromReactFlow(nodesRef.current, next));
        return next;
      });
    },
    [mode, onEdit],
  );

  const onConnect = useCallback(
    (conn: Connection) => {
      if (mode !== "edit") return;
      setEdges((prev) => {
        const next = addEdge({ ...conn, animated: true }, prev);
        onEdit?.(fromReactFlow(nodesRef.current, next));
        return next;
      });
    },
    [mode, onEdit],
  );

  const addNode = useCallback(
    (item: PaletteItem, position?: { x: number; y: number }) => {
      addCounter.current += 1;
      const id = `n_${Date.now()}_${addCounter.current}`;
      const config = { ...item.defaultConfig, label: item.label, template: item.type };
      const node: Node = {
        id,
        type: item.category,
        position: position ?? { x: 160 + addCounter.current * 36, y: 120 + addCounter.current * 28 },
        data: {
          label: item.label,
          state: "idle" satisfies StepState,
          category: item.category,
          config,
        } satisfies FlowNodeData,
      };
      setNodes((prev) => {
        const next = [...prev, node];
        onEdit?.(fromReactFlow(next, edgesRef.current));
        return next;
      });
    },
    [onEdit],
  );

  const onDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      if (mode !== "edit") return;
      const type = e.dataTransfer.getData(DRAG_KEY);
      const item = paletteItemByType(type);
      if (!item) return;
      const position = screenToFlowPosition({ x: e.clientX, y: e.clientY });
      addNode(item, position);
    },
    [mode, screenToFlowPosition, addNode],
  );

  const onDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
  }, []);

  // ReactFlow drops edges (and warns: error#004) if it first mounts into a
  // zero-size container, and never retries. Once the wrapper has real
  // dimensions, flip `measured` — it keys the ReactFlow below so it remounts
  // once against a sized container and resolves handle positions for edges.
  const [measured, setMeasured] = useState(false);
  useEffect(() => {
    const el = wrapperRef.current;
    if (!el) return;
    const check = () => {
      if (el.clientWidth > 0 && el.clientHeight > 0) setMeasured(true);
    };
    check();
    const ro = new ResizeObserver(check);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  return (
    <div style={{ display: "flex", flex: 1, minHeight: 0, minWidth: 0 }}>
      {mode === "edit" && <PaletteSidebar onAdd={addNode} />}
      <div
        ref={wrapperRef}
        style={{ flex: 1, position: "relative", minWidth: 0, minHeight: 0 }}
        onDrop={onDrop}
        onDragOver={onDragOver}
      >
        <ReactFlow
          key={measured ? "rf-sized" : "rf-init"}
          nodes={nodes}
          edges={edges}
          nodeTypes={nodeTypes}
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
          onConnect={onConnect}
          onNodeClick={(_, n) => onNodeClick?.(n.id)}
          defaultEdgeOptions={{
            type: "smoothstep",
            markerEnd: { type: MarkerType.ArrowClosed, color: "#5b6478", width: 18, height: 18 },
            style: { stroke: "#5b6478", strokeWidth: 1.6 },
          }}
          proOptions={{ hideAttribution: true }}
          fitView
          fitViewOptions={{ padding: 0.25 }}
          style={{ background: "#0f1117" }}
        >
          <Background variant={BackgroundVariant.Dots} gap={18} size={1.4} color="#2b3142" />
          <MiniMap
            pannable
            zoomable
            nodeColor={(n) =>
              accentFor((n.type as NodeCategory) ?? "action", (n.data as FlowNodeData)?.config)
            }
            nodeStrokeWidth={0}
            maskColor="rgba(8,10,16,0.7)"
            style={{ background: "#13161f", border: "1px solid #232838", borderRadius: 8 }}
          />
          <Controls
            showInteractive={false}
            style={{
              background: "#13161f",
              border: "1px solid #232838",
              borderRadius: 8,
              overflow: "hidden",
            }}
          />
        </ReactFlow>
        {measured && nodes.length === 0 && <EmptyHint mode={mode} />}
      </div>
    </div>
  );
}

function EmptyHint({ mode }: { mode: "edit" | "monitor" }) {
  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        pointerEvents: "none",
        color: "#6b7488",
        fontSize: 14,
        textAlign: "center",
      }}
    >
      <div>
        <div style={{ fontSize: 34, marginBottom: 10 }}>✦</div>
        {mode === "edit" ? (
          <>
            <div style={{ fontWeight: 600, color: "#aab2c6" }}>Build your flow</div>
            <div style={{ marginTop: 4 }}>Drag a trigger from the palette, then wire up agents.</div>
          </>
        ) : (
          <div>No nodes to monitor yet.</div>
        )}
      </div>
    </div>
  );
}

function PaletteSidebar({ onAdd }: { onAdd: (item: PaletteItem) => void }) {
  return (
    <div
      data-testid="palette"
      style={{
        width: 232,
        flexShrink: 0,
        background: "#0c0e15",
        borderRight: "1px solid #1d2230",
        padding: 12,
        overflowY: "auto",
      }}
    >
      <div
        style={{
          fontSize: 11,
          fontWeight: 700,
          letterSpacing: 1,
          textTransform: "uppercase",
          color: "#6b7488",
          marginBottom: 10,
        }}
      >
        Nodes
      </div>
      {PALETTE_GROUPS.map((group, gi) => (
        <div key={`${group.category}-${gi}`} style={{ marginBottom: 14 }}>
          <div
            style={{
              fontSize: 10,
              fontWeight: 700,
              letterSpacing: 0.6,
              textTransform: "uppercase",
              color: CATEGORY_META[group.category].color,
              marginBottom: 6,
              opacity: 0.9,
            }}
          >
            {group.title}
          </div>
          {group.items.map((it) => {
            const accent = it.accent ?? CATEGORY_META[it.category].color;
            return (
              <button
                key={it.type}
                draggable
                onDragStart={(e) => {
                  e.dataTransfer.setData(DRAG_KEY, it.type);
                  e.dataTransfer.effectAllowed = "move";
                }}
                onClick={() => onAdd(it)}
                title={it.description}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 8,
                  width: "100%",
                  marginBottom: 6,
                  padding: "7px 9px",
                  borderRadius: 9,
                  border: "1px solid #232a3a",
                  background: "linear-gradient(180deg,#161a26,#11141d)",
                  textAlign: "left",
                  cursor: "grab",
                  color: "#dfe3ee",
                }}
              >
                <span
                  style={{
                    width: 24,
                    height: 24,
                    flexShrink: 0,
                    borderRadius: 7,
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    fontSize: 13,
                    background: hexA(accent, 0.16),
                    border: `1px solid ${hexA(accent, 0.4)}`,
                  }}
                >
                  {it.icon}
                </span>
                <span style={{ fontSize: 12.5, fontWeight: 500 }}>{it.label}</span>
              </button>
            );
          })}
        </div>
      ))}
      <div style={{ fontSize: 10.5, color: "#5a6275", lineHeight: 1.5, marginTop: 4 }}>
        Drag onto the canvas or click to add. Connect nodes left→right, then wire
        data with <code style={{ color: "#9aa3b8" }}>{"{{steps.id.outputs.x}}"}</code>.
      </div>
    </div>
  );
}

// ---- Graph <-> ReactFlow conversion (config-preserving) -------------------

function toReactFlowNodes(g: Graph, rs?: RunState): Node[] {
  return g.nodes.map((n, i) => ({
    id: n.id,
    type: n.type,
    position: n.position ?? { x: 140 + i * 230, y: 120 + (i % 3) * 130 },
    data: {
      label: (n.config["label"] as string) ?? n.id,
      state: rs?.steps[n.id] ?? "idle",
      category: n.type,
      config: n.config,
    } satisfies FlowNodeData,
  }));
}

function toReactFlowEdges(g: Graph, rs?: RunState): Edge[] {
  return g.edges.map((e) => {
    const flowing =
      !rs ||
      rs.steps[e.from] === "running" ||
      rs.steps[e.to] === "running" ||
      (rs.lastFlowedEdge?.from === e.from && rs.lastFlowedEdge?.to === e.to);
    return {
      id: `${e.from}->${e.to}`,
      source: e.from,
      target: e.to,
      animated: flowing,
    };
  });
}

function fromReactFlow(nodes: Node[], edges: Edge[]): Graph {
  return {
    nodes: nodes.map((n) => {
      const d = (n.data ?? {}) as Partial<FlowNodeData>;
      const baseConfig = (d.config as Record<string, unknown>) ?? {};
      const label = d.label ?? (baseConfig["label"] as string) ?? n.id;
      return {
        id: n.id,
        type: (n.type ?? d.category ?? "action") as NodeCategory,
        config: { ...baseConfig, label },
        position: n.position,
      };
    }),
    edges: edges
      .filter((e) => e.source && e.target)
      .map((e) => ({ from: e.source, to: e.target })),
  };
}

function hexA(hex: string, alpha: number): string {
  const h = hex.replace("#", "");
  const full = h.length === 3 ? h.split("").map((c) => c + c).join("") : h;
  const r = parseInt(full.slice(0, 2), 16);
  const g = parseInt(full.slice(2, 4), 16);
  const b = parseInt(full.slice(4, 6), 16);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}
