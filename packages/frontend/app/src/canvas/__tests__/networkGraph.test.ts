import { describe, it, expect } from "vitest";
import { buildNetworkGraph, FlowLike } from "../networkGraph";

const flow = (id: string, name: string, execs: string[]): FlowLike => ({
  id,
  name,
  graph_json: {
    nodes: execs.map((e, i) => ({
      id: `n${i}`,
      type: i === 0 ? "inbound" : "action",
      config: { execution: e, label: `${e}-${i}` },
    })),
    edges: execs.slice(1).map((_, i) => ({ from: `n${i}`, to: `n${i + 1}` })),
  },
});

describe("buildNetworkGraph", () => {
  it("links workspace → flows → entry steps, and steps along data edges", () => {
    const g = buildNetworkGraph("Demo", [flow("f1", "Newsletter", ["input", "llm", "doc"])]);
    expect(g.nodes.find((n) => n.id === "workspace")?.name).toBe("Demo");
    expect(g.links).toContainEqual({ source: "workspace", target: "flow:f1", kind: "contains" });
    // n0 is the only root → hangs off the hub; n1/n2 do not.
    expect(g.links).toContainEqual({ source: "flow:f1", target: "flow:f1:n0", kind: "contains" });
    expect(g.links.filter((l) => l.kind === "contains" && l.source === "flow:f1")).toHaveLength(1);
    expect(g.links).toContainEqual({ source: "flow:f1:n0", target: "flow:f1:n1", kind: "dataflow" });
  });

  it("creates ONE shared service node used by multiple flows", () => {
    const g = buildNetworkGraph("Demo", [
      flow("f1", "A", ["input", "memory"]),
      flow("f2", "B", ["input", "memory", "m365"]),
    ]);
    expect(g.nodes.filter((n) => n.id === "svc:galvo-memory")).toHaveLength(1);
    const uses = g.links.filter((l) => l.target === "svc:galvo-memory" && l.kind === "uses");
    expect(uses).toHaveLength(2); // one from each flow's memory step
    expect(g.nodes.some((n) => n.id === "svc:m365")).toBe(true);
    expect(g.nodes.some((n) => n.id === "svc:llm")).toBe(false); // unused service absent
  });

  it("marks flow hubs with flowId for click-through", () => {
    const g = buildNetworkGraph("Demo", [flow("f9", "X", ["input"])]);
    expect(g.nodes.find((n) => n.id === "flow:f9")?.flowId).toBe("f9");
    expect(g.nodes.find((n) => n.id === "workspace")?.flowId).toBeUndefined();
  });

  it("handles an empty workspace", () => {
    const g = buildNetworkGraph("", []);
    expect(g.nodes).toHaveLength(1);
    expect(g.links).toHaveLength(0);
  });
});
