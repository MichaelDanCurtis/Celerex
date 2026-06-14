import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { BloomBackdrop } from "../BloomBackdrop";
import { GlassPanel } from "../GlassPanel";
import { GlassButton } from "../GlassButton";

describe("BloomBackdrop", () => {
  it("renders an aria-hidden bloom field with two blooms", () => {
    const { container } = render(<BloomBackdrop />);
    const root = screen.getByTestId("ov-bloom-backdrop");
    expect(root.getAttribute("aria-hidden")).toBe("true");
    expect(container.querySelectorAll(".ov-bloom").length).toBe(2);
  });
});

describe("Glass primitives", () => {
  it("GlassPanel carries the ov-glass class", () => {
    render(<GlassPanel testId="p">hi</GlassPanel>);
    expect(screen.getByTestId("p").className).toContain("ov-glass");
  });

  it("GlassButton fires onClick and tints when prominent", () => {
    let clicked = false;
    render(
      <GlassButton testId="b" prominent onClick={() => (clicked = true)}>
        Create
      </GlassButton>,
    );
    const btn = screen.getByTestId("b");
    expect(btn.style.background).toContain("ov-glass-tint");
    fireEvent.click(btn);
    expect(clicked).toBe(true);
  });
});
