import { describe, it, expect } from "vitest";
import { autoLayout } from "../layout";
import { Graph } from "../types";

function g(nodes: string[], edges: Array<[string, string]>): Graph {
  return {
    nodes: nodes.map((id) => ({ id, type: "action" as const, config: {} })),
    edges: edges.map(([from, to]) => ({ from, to })),
  };
}

describe("autoLayout", () => {
  it("lays a linear chain out left to right", () => {
    const out = autoLayout(g(["a", "b", "c"], [["a", "b"], ["b", "c"]]));
    const x = (id: string) => out.nodes.find((n) => n.id === id)!.position!.x;
    expect(x("a")).toBeLessThan(x("b"));
    expect(x("b")).toBeLessThan(x("c"));
  });

  it("uses longest path for diamonds (fan-in lands after both branches)", () => {
    // a -> b -> d and a -> c -> d, plus a direct a -> d shortcut.
    const out = autoLayout(
      g(["a", "b", "c", "d"], [["a", "b"], ["a", "c"], ["b", "d"], ["c", "d"], ["a", "d"]]),
    );
    const x = (id: string) => out.nodes.find((n) => n.id === id)!.position!.x;
    expect(x("d")).toBeGreaterThan(x("b"));
    expect(x("b")).toBe(x("c")); // same depth → same column
  });

  it("stacks same-column nodes at distinct ys", () => {
    const out = autoLayout(g(["a", "b", "c"], [["a", "b"], ["a", "c"]]));
    const b = out.nodes.find((n) => n.id === "b")!.position!;
    const c = out.nodes.find((n) => n.id === "c")!.position!;
    expect(b.x).toBe(c.x);
    expect(b.y).not.toBe(c.y);
  });

  it("survives cycles without hanging", () => {
    const out = autoLayout(g(["a", "b"], [["a", "b"], ["b", "a"]]));
    expect(out.nodes).toHaveLength(2);
    for (const n of out.nodes) expect(n.position).toBeDefined();
  });

  it("handles empty graphs", () => {
    expect(autoLayout({ nodes: [], edges: [] })).toEqual({ nodes: [], edges: [] });
  });
});
