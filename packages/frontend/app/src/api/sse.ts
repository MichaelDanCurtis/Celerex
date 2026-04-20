import { useEffect, useRef, useState } from "react";
import { API_BASE } from "./client";
import { RunState, StepState } from "../canvas/types";

export interface ServerEvent {
  event: "run_step_started" | "run_step_succeeded" | "run_step_failed" | "run_completed";
  run_id: string;
  step_id?: string;
  outputs?: unknown;
  error?: string;
  status?: string;
}

export interface StepIO {
  outputs?: unknown;
  error?: string;
}

export interface UseRunEventsResult {
  state: RunState;
  io: Record<string, StepIO>;
}

export function useRunEvents(runId: string | null): UseRunEventsResult {
  const [state, setState] = useState<RunState>({ steps: {} });
  const [io, setIo] = useState<Record<string, StepIO>>({});
  const sourceRef = useRef<EventSource | null>(null);

  useEffect(() => {
    if (!runId) return;
    const es = new EventSource(`${API_BASE}/runs/${runId}/events`);
    sourceRef.current = es;
    es.onmessage = (msg) => {
      try {
        const ev = JSON.parse(msg.data) as ServerEvent;
        setState((prev) => applyEvent(prev, ev));
        if (ev.step_id) {
          setIo((prev) => ({
            ...prev,
            [ev.step_id as string]: {
              outputs: ev.outputs ?? prev[ev.step_id as string]?.outputs,
              error: ev.error ?? prev[ev.step_id as string]?.error,
            },
          }));
        }
      } catch {
        /* ignore bad frames */
      }
    };
    return () => {
      es.close();
      sourceRef.current = null;
    };
  }, [runId]);

  return { state, io };
}

function applyEvent(prev: RunState, ev: ServerEvent): RunState {
  if (!ev.step_id) return prev;
  const s: StepState =
    ev.event === "run_step_started"
      ? "running"
      : ev.event === "run_step_succeeded"
        ? "succeeded"
        : ev.event === "run_step_failed"
          ? "failed"
          : "idle";
  return { ...prev, steps: { ...prev.steps, [ev.step_id]: s } };
}
