import { useState, useRef, useMemo, useCallback, useEffect } from "react";
import {
  getAllAgents,
  CATEGORIES,
  type AgentDef,
  type CategoryId,
} from "../data/agents";
import { SUBCATEGORIES, classifyAgent } from "../data/subcategories";

/* ------------------------------------------------------------------ */
/*  Types                                                             */
/* ------------------------------------------------------------------ */

export interface PipelineNode {
  id: string;
  agentId: string;
  agentName: string;
  agentDescription: string;
  agentCategory: "document-management" | "co-development" | "infrastructure-design";
  x: number;
  y: number;
}

export interface PipelineEdge {
  id: string;
  fromNodeId: string;
  toNodeId: string;
}

export interface Pipeline {
  id: string;
  name: string;
  nodes: PipelineNode[];
  edges: PipelineEdge[];
}

/* ------------------------------------------------------------------ */
/*  Constants                                                         */
/* ------------------------------------------------------------------ */

const CATEGORY_COLORS: Record<PipelineNode["agentCategory"], string> = {
  "document-management": "#3b82f6",
  "co-development": "#22c55e",
  "infrastructure-design": "#a855f7",
};

const CATEGORY_LABELS: Record<PipelineNode["agentCategory"], string> = {
  "document-management": "Document Management",
  "co-development": "Co-Development",
  "infrastructure-design": "Infrastructure Design",
};

const NODE_W = 200;
const NODE_H = 70;
const PORT_R = 10;
const PALETTE_LIMIT = 50;

/* ------------------------------------------------------------------ */
/*  Helpers                                                           */
/* ------------------------------------------------------------------ */

function uid(): string {
  if (typeof crypto !== "undefined" && crypto.randomUUID) {
    return crypto.randomUUID();
  }
  return Math.random().toString(36).slice(2) + Date.now().toString(36);
}

function inputPortPos(node: PipelineNode) {
  return { x: node.x, y: node.y + NODE_H / 2 };
}

function outputPortPos(node: PipelineNode) {
  return { x: node.x + NODE_W, y: node.y + NODE_H / 2 };
}

function bezierPath(x1: number, y1: number, x2: number, y2: number): string {
  const dx = Math.abs(x2 - x1) * 0.5;
  return `M ${x1} ${y1} C ${x1 + dx} ${y1}, ${x2 - dx} ${y2}, ${x2} ${y2}`;
}

/** Topological sort — returns node ids in execution order (left to right). */
function topoSort(nodes: PipelineNode[], edges: PipelineEdge[]): string[] {
  const adj = new Map<string, string[]>();
  const indeg = new Map<string, number>();
  for (const n of nodes) {
    adj.set(n.id, []);
    indeg.set(n.id, 0);
  }
  for (const e of edges) {
    adj.get(e.fromNodeId)?.push(e.toNodeId);
    indeg.set(e.toNodeId, (indeg.get(e.toNodeId) ?? 0) + 1);
  }
  const queue: string[] = [];
  for (const n of nodes) {
    if ((indeg.get(n.id) ?? 0) === 0) queue.push(n.id);
  }
  // Sort roots by x position so left-most runs first
  queue.sort((a, b) => {
    const na = nodes.find((n) => n.id === a)!;
    const nb = nodes.find((n) => n.id === b)!;
    return na.x - nb.x;
  });
  const result: string[] = [];
  while (queue.length) {
    const cur = queue.shift()!;
    result.push(cur);
    for (const next of adj.get(cur) ?? []) {
      const d = (indeg.get(next) ?? 1) - 1;
      indeg.set(next, d);
      if (d === 0) queue.push(next);
    }
  }
  return result;
}

/* ------------------------------------------------------------------ */
/*  Component                                                         */
/* ------------------------------------------------------------------ */

export function PipelineEditor() {
  /* ---- pipeline state ---- */
  const [pipelineName, setPipelineName] = useState("Untitled Pipeline");
  const [nodes, setNodes] = useState<PipelineNode[]>([]);
  const [edges, setEdges] = useState<PipelineEdge[]>([]);

  /* ---- selection ---- */
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [selectedEdgeId, setSelectedEdgeId] = useState<string | null>(null);

  /* ---- connecting state ---- */
  const [connectingFrom, setConnectingFrom] = useState<string | null>(null);
  const [mousePos, setMousePos] = useState<{ x: number; y: number } | null>(null);

  /* ---- node dragging ---- */
  const draggingNodeRef = useRef<{
    nodeId: string;
    offsetX: number;
    offsetY: number;
  } | null>(null);

  /* ---- canvas ref ---- */
  const canvasRef = useRef<HTMLDivElement>(null);

  /* ---- palette state ---- */
  const [paletteSearch, setPaletteSearch] = useState("");
  const [collapsedCats, setCollapsedCats] = useState<Record<string, boolean>>({});
  const [collapsedSubcats, setCollapsedSubcats] = useState<Record<string, boolean>>({});
  const [loadMoreCounts, setLoadMoreCounts] = useState<Record<string, number>>({});

  /* ---- run simulation ---- */
  const [running, setRunning] = useState(false);
  const [litNodeId, setLitNodeId] = useState<string | null>(null);

  /* ---- palette agents grouped by category → subcategory ---- */
  const paletteData = useMemo(() => {
    const q = paletteSearch.toLowerCase().trim();
    const allAgents = getAllAgents();
    const result: {
      catId: CategoryId;
      catLabel: string;
      subcats: { id: string; label: string; agents: AgentDef[] }[];
      totalCount: number;
    }[] = [];

    for (const cat of CATEGORIES) {
      const catAgents = allAgents.filter((a) => {
        if (a.category !== cat.id) return false;
        if (!q) return true;
        return (
          a.name.toLowerCase().includes(q) ||
          a.description.toLowerCase().includes(q)
        );
      });

      const subs = SUBCATEGORIES[cat.id] ?? [];
      const bySubcat = new Map<string, AgentDef[]>();
      const otherAgents: AgentDef[] = [];

      for (const agent of catAgents) {
        const subId = classifyAgent(agent.name, agent.description, cat.id);
        if (subId === "other") {
          otherAgents.push(agent);
        } else {
          if (!bySubcat.has(subId)) bySubcat.set(subId, []);
          bySubcat.get(subId)!.push(agent);
        }
      }

      const subcats: { id: string; label: string; agents: AgentDef[] }[] = [];
      for (const sub of subs) {
        const agents = bySubcat.get(sub.id);
        if (agents && agents.length > 0) {
          subcats.push({ id: sub.id, label: sub.label, agents });
        }
      }
      if (otherAgents.length > 0) {
        subcats.push({ id: "other", label: "Other", agents: otherAgents });
      }

      result.push({
        catId: cat.id,
        catLabel: cat.label,
        subcats,
        totalCount: catAgents.length,
      });
    }

    return result;
  }, [paletteSearch]);

  /* ---- keyboard delete ---- */
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (
        e.key === "Delete" ||
        e.key === "Backspace"
      ) {
        // Don't intercept if user is typing in an input
        const tag = (e.target as HTMLElement)?.tagName;
        if (tag === "INPUT" || tag === "TEXTAREA") return;

        if (selectedNodeId) {
          setNodes((prev) => prev.filter((n) => n.id !== selectedNodeId));
          setEdges((prev) =>
            prev.filter(
              (ed) =>
                ed.fromNodeId !== selectedNodeId &&
                ed.toNodeId !== selectedNodeId
            )
          );
          setSelectedNodeId(null);
          e.preventDefault();
        } else if (selectedEdgeId) {
          setEdges((prev) => prev.filter((ed) => ed.id !== selectedEdgeId));
          setSelectedEdgeId(null);
          e.preventDefault();
        }
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [selectedNodeId, selectedEdgeId]);

  /* ---- canvas position helper ---- */
  const canvasXY = useCallback(
    (clientX: number, clientY: number) => {
      const rect = canvasRef.current?.getBoundingClientRect();
      if (!rect) return { x: clientX, y: clientY };
      return {
        x: clientX - rect.left + (canvasRef.current?.scrollLeft ?? 0),
        y: clientY - rect.top + (canvasRef.current?.scrollTop ?? 0),
      };
    },
    []
  );

  /* ---- drop from palette ---- */
  const handleCanvasDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
  }, []);

  const handleCanvasDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      const data = e.dataTransfer.getData("application/celerex-agent");
      if (!data) return;
      const agent: AgentDef = JSON.parse(data);
      const pos = canvasXY(e.clientX, e.clientY);
      const newNode: PipelineNode = {
        id: uid(),
        agentId: agent.id,
        agentName: agent.name,
        agentDescription: agent.description,
        agentCategory: agent.category,
        x: pos.x - NODE_W / 2,
        y: pos.y - NODE_H / 2,
      };
      setNodes((prev) => [...prev, newNode]);
    },
    [canvasXY]
  );

  /* ---- node mouse drag ---- */
  const handleNodeMouseDown = useCallback(
    (e: React.MouseEvent, nodeId: string) => {
      // Ignore if clicking a port
      if ((e.target as HTMLElement).dataset.port) return;
      e.stopPropagation();
      const node = nodes.find((n) => n.id === nodeId);
      if (!node) return;
      const pos = canvasXY(e.clientX, e.clientY);
      draggingNodeRef.current = {
        nodeId,
        offsetX: pos.x - node.x,
        offsetY: pos.y - node.y,
      };
      setSelectedNodeId(nodeId);
      setSelectedEdgeId(null);
    },
    [nodes, canvasXY]
  );

  const handleCanvasMouseMove = useCallback(
    (e: React.MouseEvent) => {
      // Node dragging
      if (draggingNodeRef.current) {
        const pos = canvasXY(e.clientX, e.clientY);
        const { nodeId, offsetX, offsetY } = draggingNodeRef.current;
        setNodes((prev) =>
          prev.map((n) =>
            n.id === nodeId
              ? { ...n, x: pos.x - offsetX, y: pos.y - offsetY }
              : n
          )
        );
      }
      // Connecting wire follow
      if (connectingFrom) {
        const pos = canvasXY(e.clientX, e.clientY);
        setMousePos(pos);
      }
    },
    [canvasXY, connectingFrom]
  );

  const handleCanvasMouseUp = useCallback(() => {
    draggingNodeRef.current = null;
  }, []);

  /* ---- port click: connecting ---- */
  const handleOutputPortClick = useCallback(
    (e: React.MouseEvent, nodeId: string) => {
      e.stopPropagation();
      setConnectingFrom(nodeId);
      const node = nodes.find((n) => n.id === nodeId);
      if (node) {
        setMousePos(outputPortPos(node));
      }
    },
    [nodes]
  );

  const handleInputPortClick = useCallback(
    (e: React.MouseEvent, nodeId: string) => {
      e.stopPropagation();
      if (connectingFrom && connectingFrom !== nodeId) {
        // Check for duplicate edge
        const exists = edges.some(
          (ed) => ed.fromNodeId === connectingFrom && ed.toNodeId === nodeId
        );
        if (!exists) {
          setEdges((prev) => [
            ...prev,
            { id: uid(), fromNodeId: connectingFrom, toNodeId: nodeId },
          ]);
        }
      }
      setConnectingFrom(null);
      setMousePos(null);
    },
    [connectingFrom, edges]
  );

  /* ---- canvas click: deselect / cancel connecting ---- */
  const handleCanvasClick = useCallback(
    (e: React.MouseEvent) => {
      if ((e.target as HTMLElement).closest("[data-pipeline-node]")) return;
      setSelectedNodeId(null);
      setSelectedEdgeId(null);
      if (connectingFrom) {
        setConnectingFrom(null);
        setMousePos(null);
      }
    },
    [connectingFrom]
  );

  /* ---- run pipeline ---- */
  const handleRun = useCallback(async () => {
    if (running || nodes.length === 0) return;
    setRunning(true);
    const order = topoSort(nodes, edges);
    for (const nid of order) {
      setLitNodeId(nid);
      await new Promise((r) => setTimeout(r, 500));
    }
    setLitNodeId(null);
    setRunning(false);
  }, [running, nodes, edges]);

  /* ---- clear ---- */
  const handleClear = useCallback(() => {
    setNodes([]);
    setEdges([]);
    setSelectedNodeId(null);
    setSelectedEdgeId(null);
    setConnectingFrom(null);
    setMousePos(null);
  }, []);

  /* ---- edge click ---- */
  const handleEdgeClick = useCallback(
    (e: React.MouseEvent, edgeId: string) => {
      e.stopPropagation();
      setSelectedEdgeId(edgeId);
      setSelectedNodeId(null);
    },
    []
  );

  /* ================================================================ */
  /*  Render                                                          */
  /* ================================================================ */

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100%",
        width: "100%",
        background: "var(--celerex-bg-primary)",
        color: "var(--celerex-text-primary)",
        fontFamily: "inherit",
      }}
    >
      {/* ============ TOOLBAR ============ */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 12,
          padding: "8px 16px",
          borderBottom: "1px solid var(--celerex-border)",
          background: "var(--celerex-bg-secondary)",
          flexShrink: 0,
          minHeight: 48,
        }}
      >
        <input
          value={pipelineName}
          onChange={(e) => setPipelineName(e.target.value)}
          style={{
            background: "transparent",
            border: "1px solid transparent",
            borderRadius: 4,
            padding: "4px 8px",
            fontSize: 16,
            fontWeight: 700,
            color: "var(--celerex-text-primary)",
            outline: "none",
            minWidth: 180,
          }}
          onFocus={(e) => {
            (e.target as HTMLInputElement).style.borderColor =
              "var(--celerex-accent)";
          }}
          onBlur={(e) => {
            (e.target as HTMLInputElement).style.borderColor = "transparent";
          }}
        />

        <span
          style={{
            fontSize: 12,
            padding: "2px 8px",
            borderRadius: 10,
            background: "var(--celerex-accent)",
            color: "#fff",
            fontWeight: 600,
            whiteSpace: "nowrap",
          }}
        >
          {nodes.length} node{nodes.length !== 1 ? "s" : ""}
        </span>

        <div style={{ flex: 1 }} />

        <button
          onClick={handleRun}
          disabled={running || nodes.length === 0}
          style={{
            padding: "6px 16px",
            borderRadius: 6,
            border: "none",
            background: running ? "#16a34a" : "#22c55e",
            color: "#fff",
            fontWeight: 600,
            fontSize: 13,
            cursor: running || nodes.length === 0 ? "not-allowed" : "pointer",
            opacity: nodes.length === 0 ? 0.5 : 1,
            display: "flex",
            alignItems: "center",
            gap: 6,
          }}
        >
          {running ? "Running..." : "\u25B6 Run Pipeline"}
        </button>

        <button
          onClick={handleClear}
          style={{
            padding: "6px 14px",
            borderRadius: 6,
            border: "1px solid var(--celerex-border)",
            background: "transparent",
            color: "var(--celerex-text-secondary)",
            fontWeight: 600,
            fontSize: 13,
            cursor: "pointer",
          }}
        >
          Clear
        </button>

        <button
          onClick={() => {
            const pipeline: Pipeline = {
              id: uid(),
              name: pipelineName,
              nodes,
              edges,
            };
            // eslint-disable-next-line no-console
            console.log("Pipeline saved:", pipeline);
          }}
          style={{
            padding: "6px 14px",
            borderRadius: 6,
            border: "1px solid var(--celerex-border)",
            background: "var(--celerex-accent)",
            color: "#fff",
            fontWeight: 600,
            fontSize: 13,
            cursor: "pointer",
          }}
        >
          Save
        </button>
      </div>

      {/* ============ BODY (sidebar + canvas) ============ */}
      <div style={{ display: "flex", flex: 1, overflow: "hidden" }}>
        {/* ---- LEFT SIDEBAR: PALETTE ---- */}
        <div
          style={{
            width: 250,
            flexShrink: 0,
            background: "var(--celerex-bg-sidebar)",
            borderRight: "1px solid var(--celerex-border)",
            display: "flex",
            flexDirection: "column",
            overflow: "hidden",
          }}
        >
          {/* search */}
          <div style={{ padding: "12px 12px 8px" }}>
            <input
              placeholder="Search agents..."
              value={paletteSearch}
              onChange={(e) => {
                setPaletteSearch(e.target.value);
                setLoadMoreCounts({});
              }}
              style={{
                width: "100%",
                boxSizing: "border-box",
                padding: "6px 10px",
                borderRadius: 6,
                border: "1px solid var(--celerex-border)",
                background: "var(--celerex-bg-primary)",
                color: "var(--celerex-text-primary)",
                fontSize: 13,
                outline: "none",
              }}
            />
          </div>

          {/* categories → subcategories */}
          <div style={{ flex: 1, overflowY: "auto", padding: "0 12px 12px" }}>
            {paletteData.map((catGroup) => {
              const collapsed = !!collapsedCats[catGroup.catId];
              const color = CATEGORY_COLORS[catGroup.catId as PipelineNode["agentCategory"]];

              return (
                <div key={catGroup.catId} style={{ marginTop: 10 }}>
                  {/* category header */}
                  <button
                    onClick={() =>
                      setCollapsedCats((p) => ({
                        ...p,
                        [catGroup.catId]: !p[catGroup.catId],
                      }))
                    }
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: 6,
                      width: "100%",
                      background: "none",
                      border: "none",
                      cursor: "pointer",
                      padding: "4px 0",
                      color: "var(--celerex-text-primary)",
                      fontSize: 12,
                      fontWeight: 700,
                      textTransform: "uppercase",
                      letterSpacing: "0.04em",
                    }}
                  >
                    <span
                      style={{
                        display: "inline-block",
                        width: 8,
                        height: 8,
                        borderRadius: 2,
                        background: color,
                        flexShrink: 0,
                      }}
                    />
                    <span style={{ flex: 1, textAlign: "left" }}>
                      {catGroup.catLabel}
                    </span>
                    <span
                      style={{
                        fontSize: 11,
                        color: "var(--celerex-text-secondary)",
                        fontWeight: 500,
                      }}
                    >
                      {catGroup.totalCount}
                    </span>
                    <span
                      style={{
                        fontSize: 10,
                        transform: collapsed
                          ? "rotate(-90deg)"
                          : "rotate(0deg)",
                        transition: "transform 0.15s",
                      }}
                    >
                      \u25BC
                    </span>
                  </button>

                  {!collapsed &&
                    catGroup.subcats.map((subcat) => {
                      const subKey = `${catGroup.catId}:${subcat.id}`;
                      const subCollapsed = !!collapsedSubcats[subKey];
                      const limit =
                        (loadMoreCounts[subKey] ?? 1) * PALETTE_LIMIT;
                      const visible = subcat.agents.slice(0, limit);
                      const hasMore = subcat.agents.length > limit;

                      return (
                        <div key={subcat.id} style={{ marginLeft: 10, marginTop: 6 }}>
                          {/* subcategory header */}
                          <button
                            onClick={() =>
                              setCollapsedSubcats((p) => ({
                                ...p,
                                [subKey]: !p[subKey],
                              }))
                            }
                            style={{
                              display: "flex",
                              alignItems: "center",
                              gap: 5,
                              width: "100%",
                              background: "none",
                              border: "none",
                              cursor: "pointer",
                              padding: "2px 0",
                              color: "var(--celerex-text-secondary)",
                              fontSize: 11,
                              fontWeight: 600,
                            }}
                          >
                            <span
                              style={{
                                fontSize: 8,
                                transform: subCollapsed
                                  ? "rotate(-90deg)"
                                  : "rotate(0deg)",
                                transition: "transform 0.15s",
                              }}
                            >
                              \u25BC
                            </span>
                            <span style={{ flex: 1, textAlign: "left" }}>
                              {subcat.label}
                            </span>
                            <span
                              style={{
                                fontSize: 10,
                                color: "var(--celerex-text-secondary)",
                                opacity: 0.7,
                              }}
                            >
                              {subcat.agents.length}
                            </span>
                          </button>

                          {!subCollapsed && (
                            <div
                              style={{
                                display: "flex",
                                flexWrap: "wrap",
                                gap: 4,
                                marginTop: 3,
                                marginLeft: 4,
                              }}
                            >
                              {visible.map((agent) => (
                                <div
                                  key={agent.id}
                                  draggable
                                  onDragStart={(e) => {
                                    e.dataTransfer.setData(
                                      "application/celerex-agent",
                                      JSON.stringify(agent)
                                    );
                                    e.dataTransfer.effectAllowed = "copy";
                                  }}
                                  title={agent.description}
                                  style={{
                                    padding: "3px 9px",
                                    borderRadius: 12,
                                    fontSize: 11,
                                    fontWeight: 500,
                                    background: color + "1a",
                                    color: color,
                                    border: `1px solid ${color}44`,
                                    cursor: "grab",
                                    whiteSpace: "nowrap",
                                    overflow: "hidden",
                                    textOverflow: "ellipsis",
                                    maxWidth: "100%",
                                    userSelect: "none",
                                  }}
                                >
                                  {agent.name}
                                </div>
                              ))}

                              {hasMore && (
                                <button
                                  onClick={() =>
                                    setLoadMoreCounts((p) => ({
                                      ...p,
                                      [subKey]: (p[subKey] ?? 1) + 1,
                                    }))
                                  }
                                  style={{
                                    padding: "3px 10px",
                                    borderRadius: 12,
                                    fontSize: 11,
                                    fontWeight: 600,
                                    background: "var(--celerex-bg-primary)",
                                    color: "var(--celerex-text-secondary)",
                                    border: "1px solid var(--celerex-border)",
                                    cursor: "pointer",
                                  }}
                                >
                                  +{subcat.agents.length - limit} more
                                </button>
                              )}
                            </div>
                          )}
                        </div>
                      );
                    })}
                </div>
              );
            })}
          </div>
        </div>

        {/* ---- CENTER: CANVAS ---- */}
        <div
          ref={canvasRef}
          onClick={handleCanvasClick}
          onDragOver={handleCanvasDragOver}
          onDrop={handleCanvasDrop}
          onMouseMove={handleCanvasMouseMove}
          onMouseUp={handleCanvasMouseUp}
          onMouseLeave={handleCanvasMouseUp}
          style={{
            flex: 1,
            position: "relative",
            overflow: "auto",
            cursor: connectingFrom ? "crosshair" : "default",
            background:
              "radial-gradient(circle, var(--celerex-border) 1px, transparent 1px)",
            backgroundSize: "24px 24px",
            backgroundPosition: "12px 12px",
          }}
        >
          {/* SVG edge layer */}
          <svg
            style={{
              position: "absolute",
              top: 0,
              left: 0,
              width: "100%",
              height: "100%",
              pointerEvents: "none",
              overflow: "visible",
            }}
          >
            {/* Existing edges */}
            {edges.map((edge) => {
              const fromNode = nodes.find((n) => n.id === edge.fromNodeId);
              const toNode = nodes.find((n) => n.id === edge.toNodeId);
              if (!fromNode || !toNode) return null;
              const from = outputPortPos(fromNode);
              const to = inputPortPos(toNode);
              const isSelected = selectedEdgeId === edge.id;
              const isLit =
                litNodeId !== null &&
                (edge.fromNodeId === litNodeId || edge.toNodeId === litNodeId);
              return (
                <g key={edge.id}>
                  {/* Fat invisible hit area */}
                  <path
                    d={bezierPath(from.x, from.y, to.x, to.y)}
                    fill="none"
                    stroke="transparent"
                    strokeWidth={14}
                    style={{ pointerEvents: "stroke", cursor: "pointer" }}
                    onClick={(e) => handleEdgeClick(e as unknown as React.MouseEvent, edge.id)}
                  />
                  <path
                    d={bezierPath(from.x, from.y, to.x, to.y)}
                    fill="none"
                    stroke={
                      isLit
                        ? "#22c55e"
                        : isSelected
                        ? "var(--celerex-accent)"
                        : "var(--celerex-text-secondary)"
                    }
                    strokeWidth={isSelected ? 2.5 : 2}
                    strokeDasharray={isLit ? "none" : "none"}
                    style={{ pointerEvents: "none" }}
                  />
                </g>
              );
            })}

            {/* Temporary connecting wire */}
            {connectingFrom && mousePos && (() => {
              const fromNode = nodes.find((n) => n.id === connectingFrom);
              if (!fromNode) return null;
              const from = outputPortPos(fromNode);
              return (
                <path
                  d={bezierPath(from.x, from.y, mousePos.x, mousePos.y)}
                  fill="none"
                  stroke="var(--celerex-accent)"
                  strokeWidth={2}
                  strokeDasharray="6 4"
                  style={{ pointerEvents: "none" }}
                />
              );
            })()}
          </svg>

          {/* Nodes */}
          {nodes.map((node) => {
            const color = CATEGORY_COLORS[node.agentCategory];
            const isSelected = selectedNodeId === node.id;
            const isLit = litNodeId === node.id;
            const isConnectTarget =
              connectingFrom !== null && connectingFrom !== node.id;

            return (
              <div
                key={node.id}
                data-pipeline-node="true"
                onMouseDown={(e) => handleNodeMouseDown(e, node.id)}
                onClick={(e) => {
                  e.stopPropagation();
                  if (!connectingFrom) {
                    setSelectedNodeId(node.id);
                    setSelectedEdgeId(null);
                  }
                }}
                style={{
                  position: "absolute",
                  left: node.x,
                  top: node.y,
                  width: NODE_W,
                  height: NODE_H,
                  borderRadius: 10,
                  background: isLit
                    ? "#22c55e18"
                    : "var(--celerex-bg-secondary)",
                  border: isLit
                    ? "2px solid #22c55e"
                    : isSelected
                    ? "2px solid var(--celerex-accent)"
                    : "1px solid var(--celerex-border)",
                  display: "flex",
                  alignItems: "center",
                  cursor: "grab",
                  userSelect: "none",
                  boxShadow: isSelected
                    ? "0 0 0 3px var(--celerex-accent)33"
                    : isLit
                    ? "0 0 12px #22c55e55"
                    : "0 1px 4px rgba(0,0,0,0.10)",
                  transition: "box-shadow 0.15s, border-color 0.15s, background 0.2s",
                  zIndex: isSelected ? 10 : 1,
                }}
              >
                {/* Category color stripe */}
                <div
                  style={{
                    width: 5,
                    height: "100%",
                    borderRadius: "10px 0 0 10px",
                    background: color,
                    flexShrink: 0,
                  }}
                />

                {/* Input port (left) */}
                <div
                  data-port="input"
                  onClick={(e) => handleInputPortClick(e, node.id)}
                  style={{
                    position: "absolute",
                    left: -PORT_R,
                    top: NODE_H / 2 - PORT_R,
                    width: PORT_R * 2,
                    height: PORT_R * 2,
                    borderRadius: "50%",
                    background: isConnectTarget
                      ? "var(--celerex-accent)"
                      : "var(--celerex-bg-primary)",
                    border: `2px solid ${
                      isConnectTarget ? "var(--celerex-accent)" : color
                    }`,
                    cursor: "pointer",
                    zIndex: 20,
                    transition: "background 0.15s, transform 0.1s",
                    transform: isConnectTarget ? "scale(1.3)" : "scale(1)",
                  }}
                />

                {/* Output port (right) */}
                <div
                  data-port="output"
                  onClick={(e) => handleOutputPortClick(e, node.id)}
                  style={{
                    position: "absolute",
                    right: -PORT_R,
                    top: NODE_H / 2 - PORT_R,
                    width: PORT_R * 2,
                    height: PORT_R * 2,
                    borderRadius: "50%",
                    background:
                      connectingFrom === node.id
                        ? "var(--celerex-accent)"
                        : "var(--celerex-bg-primary)",
                    border: `2px solid ${
                      connectingFrom === node.id
                        ? "var(--celerex-accent)"
                        : color
                    }`,
                    cursor: "pointer",
                    zIndex: 20,
                    transition: "background 0.15s",
                  }}
                />

                {/* Content */}
                <div
                  style={{
                    flex: 1,
                    padding: "8px 12px",
                    overflow: "hidden",
                    minWidth: 0,
                  }}
                >
                  <div
                    style={{
                      fontWeight: 700,
                      fontSize: 13,
                      lineHeight: "18px",
                      whiteSpace: "nowrap",
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      color: "var(--celerex-text-primary)",
                    }}
                  >
                    {node.agentName}
                  </div>
                  <div
                    style={{
                      fontSize: 10,
                      color: color,
                      fontWeight: 600,
                      marginTop: 2,
                      whiteSpace: "nowrap",
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                    }}
                  >
                    {CATEGORY_LABELS[node.agentCategory]}
                  </div>
                </div>
              </div>
            );
          })}

          {/* Empty state */}
          {nodes.length === 0 && (
            <div
              style={{
                position: "absolute",
                top: "50%",
                left: "50%",
                transform: "translate(-50%, -50%)",
                textAlign: "center",
                color: "var(--celerex-text-secondary)",
                pointerEvents: "none",
                userSelect: "none",
              }}
            >
              <div style={{ fontSize: 40, marginBottom: 12, opacity: 0.3 }}>
                +
              </div>
              <div style={{ fontSize: 14, fontWeight: 600 }}>
                Drag agents from the palette to build your pipeline
              </div>
              <div style={{ fontSize: 12, marginTop: 4, opacity: 0.7 }}>
                Connect output ports to input ports to create data flows
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export default PipelineEditor;
