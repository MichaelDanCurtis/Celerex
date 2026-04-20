import { render, screen } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { BuilderPage } from "../BuilderPage";

vi.mock("../../api/client", () => ({
  createMember: vi.fn(async () => ({
    id: "m1",
    project_id: "p1",
    name: "x",
    description: "",
    graph_json: { nodes: [], edges: [] },
    provider_config_json: {},
    version: 1,
  })),
  runMemberNow: vi.fn(async () => ({ run_id: "r1" })),
}));

describe("BuilderPage", () => {
  it("mounts with Run now button", () => {
    render(<BuilderPage projectId="p1" token="tok" />);
    expect(screen.getByTestId("run-now")).toBeTruthy();
  });
});
