import { renderHook, act } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { useRunEvents } from "../sse";

class MockEventSource {
  onmessage: ((e: MessageEvent) => void) | null = null;
  close = vi.fn();
  static last: MockEventSource | null = null;
  constructor(public url: string) {
    MockEventSource.last = this;
  }
}
(globalThis as unknown as { EventSource: typeof MockEventSource }).EventSource =
  MockEventSource;

describe("useRunEvents", () => {
  beforeEach(() => {
    MockEventSource.last = null;
  });

  it("updates step state on run_step_succeeded", () => {
    const { result } = renderHook(() => useRunEvents("r1"));
    const es = MockEventSource.last as MockEventSource;
    act(() => {
      es.onmessage?.(
        new MessageEvent("message", {
          data: JSON.stringify({
            event: "run_step_succeeded",
            run_id: "r1",
            step_id: "a",
            outputs: { hello: "world" },
          }),
        }),
      );
    });
    expect(result.current.state.steps.a).toBe("succeeded");
    expect(result.current.io.a?.outputs).toEqual({ hello: "world" });
  });

  it("flips to running then failed", () => {
    const { result } = renderHook(() => useRunEvents("r2"));
    const es = MockEventSource.last as MockEventSource;
    act(() => {
      es.onmessage?.(
        new MessageEvent("message", {
          data: JSON.stringify({ event: "run_step_started", run_id: "r2", step_id: "b" }),
        }),
      );
    });
    expect(result.current.state.steps.b).toBe("running");
    act(() => {
      es.onmessage?.(
        new MessageEvent("message", {
          data: JSON.stringify({
            event: "run_step_failed",
            run_id: "r2",
            step_id: "b",
            error: "boom",
          }),
        }),
      );
    });
    expect(result.current.state.steps.b).toBe("failed");
    expect(result.current.io.b?.error).toBe("boom");
  });
});
