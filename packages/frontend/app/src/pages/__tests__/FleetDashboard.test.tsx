import { render, screen, waitFor } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { Provider } from "jotai";
import { FleetDashboard } from "../FleetDashboard";

vi.mock("../../api/client", () => ({
  API_BASE: "http://test",
  fetchFleet: vi.fn(async () => [
    {
      id: "m1",
      name: "Jean",
      project_id: "p1",
      last_status: "succeeded",
      next_fire_at: "2026-04-20T13:00:00Z",
      running: false,
    },
  ]),
}));

describe("FleetDashboard", () => {
  it("renders a row per member", async () => {
    render(
      <Provider>
        <FleetDashboard token="tok" />
      </Provider>,
    );
    await waitFor(() => expect(screen.getByText("Jean")).toBeTruthy());
    expect(screen.getByText("succeeded")).toBeTruthy();
  });
});
