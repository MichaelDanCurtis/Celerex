import { CATEGORY_META } from "./palette";
import type { NodeCategory } from "./types";

/** Minimal flow shape the builder needs (subset of ApiProjectMember). */
export interface FlowLike {
  id: string;
  name: string;
  graph_json: {
    nodes: { id: string; type: string; config: Record<string, unknown> }[];
    edges: { from: string; to: string }[];
  };
}

export interface NetNode {
  id: string;
  name: string;
  group: "workspace" | "flow" | "step" | "service";
  color: string;
  val: number;
  /** For flow hubs: the member id to open in the Builder. */
  flowId?: string;
}

export interface NetLink {
  source: string;
  target: string;
  kind: "contains" | "dataflow" | "uses";
}

export interface NetGraph {
  nodes: NetNode[];
  links: NetLink[];
}

/** Shared external services a flow can use, keyed by node execution type. */
const SERVICES: Record<string, { id: string; name: string; color: string }> = {
  memory: { id: "svc:galvo-memory", name: "galvo-memory", color: "#2dd4bf" },
  galvo: { id: "svc:galvo", name: "Galvo", color: "#818cf8" },
  m365: { id: "svc:m365", name: "Microsoft 365", color: "#38bdf8" },
  llm: { id: "svc:llm", name: "LLM providers", color: "#a78bfa" },
};

/**
 * Build the 3D agent-network graph: the workspace at the center, each flow as
 * a hub, each flow's steps as satellites wired by their real data edges, and
 * shared services (galvo-memory / Galvo / M365 / LLM) as cross-flow anchors —
 * so federation is literally visible as shared structure between flows.
 */
export function buildNetworkGraph(workspaceName: string, flows: FlowLike[]): NetGraph {
  const nodes: NetNode[] = [];
  const links: NetLink[] = [];
  const usedServices = new Set<string>();

  nodes.push({
    id: "workspace",
    name: workspaceName || "Workspace",
    group: "workspace",
    color: "#f8fafc",
    val: 26,
  });

  for (const flow of flows) {
    const hubId = `flow:${flow.id}`;
    nodes.push({
      id: hubId,
      name: flow.name,
      group: "flow",
      color: "#60a5fa",
      val: 12,
      flowId: flow.id,
    });
    links.push({ source: "workspace", target: hubId, kind: "contains" });

    const stepIds = new Set(flow.graph_json.nodes.map((n) => n.id));
    const hasIncoming = new Set(
      flow.graph_json.edges.filter((e) => stepIds.has(e.from)).map((e) => e.to),
    );

    for (const step of flow.graph_json.nodes) {
      const stepNodeId = `${hubId}:${step.id}`;
      const category = (step.type as NodeCategory) in CATEGORY_META ? (step.type as NodeCategory) : "action";
      const label = (step.config["label"] as string) ?? step.id;
      nodes.push({
        id: stepNodeId,
        name: `${flow.name} · ${label}`,
        group: "step",
        color: CATEGORY_META[category].color,
        val: 3,
      });
      // Entry steps hang off the flow hub; the rest connect via real edges.
      if (!hasIncoming.has(step.id)) {
        links.push({ source: hubId, target: stepNodeId, kind: "contains" });
      }
      // Service usage — federation made visible.
      const exec = String(step.config["execution"] ?? "");
      const svc = SERVICES[exec];
      if (svc) {
        if (!usedServices.has(svc.id)) {
          usedServices.add(svc.id);
          nodes.push({ id: svc.id, name: svc.name, group: "service", color: svc.color, val: 16 });
        }
        links.push({ source: stepNodeId, target: svc.id, kind: "uses" });
      }
    }

    for (const e of flow.graph_json.edges) {
      if (!stepIds.has(e.from) || !stepIds.has(e.to)) continue;
      links.push({ source: `${hubId}:${e.from}`, target: `${hubId}:${e.to}`, kind: "dataflow" });
    }
  }

  return { nodes, links };
}
