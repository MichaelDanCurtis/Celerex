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
