export type NodeCategory = "inbound" | "action" | "filter" | "outbound";

export interface GraphNode {
  id: string;
  type: NodeCategory;
  config: Record<string, unknown>;
  position?: { x: number; y: number };
}

export interface GraphEdge {
  from: string;
  to: string;
}

export interface Graph {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export type StepState = "idle" | "running" | "succeeded" | "failed";

export interface RunState {
  steps: Record<string, StepState>;
  lastFlowedEdge?: { from: string; to: string };
}

/**
 * The `data` payload carried by every ReactFlow node. Crucially it holds the
 * FULL node `config` (not just a label) so structural canvas edits — move,
 * connect, add — round-trip back to the Graph without dropping configuration.
 * This is the fix for the bug where canvas-built nodes had empty config and
 * couldn't execute.
 */
export interface FlowNodeData extends Record<string, unknown> {
  label: string;
  state?: StepState;
  category: NodeCategory;
  config: Record<string, unknown>;
  /** Edit-mode toolbar callbacks, injected by the canvas (absent in monitor mode). */
  onDelete?: (id: string) => void;
  onDuplicate?: (id: string) => void;
}
