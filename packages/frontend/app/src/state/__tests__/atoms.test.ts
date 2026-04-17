import { describe, it, expect } from "vitest";
import { createStore } from "jotai";
import { engagementsAtom } from "../atoms";

describe("engagementsAtom", () => {
  it("starts with at least one default engagement", () => {
    const store = createStore();
    const value = store.get(engagementsAtom);
    expect(Array.isArray(value)).toBe(true);
    expect(value.length).toBeGreaterThanOrEqual(1);
  });
});
