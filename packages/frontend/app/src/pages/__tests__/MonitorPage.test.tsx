import { render, screen } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { MonitorPage } from "../MonitorPage";

vi.mock("../../api/client", () => ({
  getMember: vi.fn(async () => ({
    id: "m1",
    project_id: "p1",
    name: "x",
    description: "",
    graph_json: { nodes: [], edges: [] },
    provider_config_json: {},
    version: 1,
  })),
  API_BASE: "http://test/api",
}));

class MockEventSource {
  onmessage: ((e: MessageEvent) => void) | null = null;
  close = vi.fn();
  constructor(public url: string) {}
}
(globalThis as unknown as { EventSource: typeof MockEventSource }).EventSource =
  MockEventSource;

describe("MonitorPage", () => {
  it("mounts with a run selector", () => {
    render(<MonitorPage />);
    expect(screen.getByTestId("run-selector")).toBeTruthy();
  });
});
