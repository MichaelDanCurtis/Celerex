import { describe, it, expect } from "vitest";
import { createStore } from "jotai";
import { projectsAtom, activeProjectIdAtom, engagementsForActiveProjectAtom, engagementsAtom } from "../atoms";

describe("projects", () => {
  it("filters engagements by active project", () => {
    const store = createStore();
    store.set(projectsAtom, [{ id: "p1", name: "Newsletter", engagementId: "e1" }]);
    store.set(engagementsAtom, [
      { id: "e1", projectId: "p1", name: "Acme", docs: [], createdAt: 0 },
      { id: "e2", projectId: "p2", name: "Other", docs: [], createdAt: 0 },
    ]);
    store.set(activeProjectIdAtom, "p1");
    const visible = store.get(engagementsForActiveProjectAtom);
    expect(visible.map(e => e.id)).toEqual(["e1"]);
  });
});
