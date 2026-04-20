import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { AgentBuilderCanvas } from "../AgentBuilderCanvas";

describe("AgentBuilderCanvas", () => {
  it("renders the palette in edit mode", () => {
    render(<AgentBuilderCanvas mode="edit" graph={{ nodes: [], edges: [] }} />);
    expect(screen.getByTestId("palette")).toBeTruthy();
  });

  it("omits the palette in monitor mode", () => {
    render(<AgentBuilderCanvas mode="monitor" graph={{ nodes: [], edges: [] }} />);
    expect(screen.queryByTestId("palette")).toBeNull();
  });
});
