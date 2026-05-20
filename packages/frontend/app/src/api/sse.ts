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

export interface RunEventLogEntry {
  /** monotonically increasing id so React can key the list */
  seq: number;
  /** wall-clock receipt time in epoch millis */
  ts: number;
  ev: ServerEvent;
}

export interface UseRunEventsResult {
  state: RunState;
  io: Record<string, StepIO>;
  /** Ordered log of every event received for this run (oldest first). */
  events: RunEventLogEntry[];
  /** True once the connection is closed (completed or errored). */
  closed: boolean;
}

export function useRunEvents(runId: string | null): UseRunEventsResult {
  const [state, setState] = useState<RunState>({ steps: {} });
  const [io, setIo] = useState<Record<string, StepIO>>({});
  const [events, setEvents] = useState<RunEventLogEntry[]>([]);
  const [closed, setClosed] = useState<boolean>(false);
  const sourceRef = useRef<EventSource | null>(null);
  const seqRef = useRef<number>(0);

  useEffect(() => {
    if (!runId) {
      setState({ steps: {} });
      setIo({});
      setEvents([]);
      setClosed(false);
      return;
    }
    // Reset per-run state when switching to a new run.
    seqRef.current = 0;
    setState({ steps: {} });
    setIo({});
    setEvents([]);
    setClosed(false);

    const es = new EventSource(`${API_BASE}/runs/${runId}/events`);
    sourceRef.current = es;

    const closeAndFlag = () => {
      es.close();
      sourceRef.current = null;
      setClosed(true);
    };

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
        setEvents((prev) => [
          ...prev,
          { seq: ++seqRef.current, ts: Date.now(), ev },
        ]);
        if (ev.event === "run_completed") {
          // Backend signals end-of-stream; close so we don't leak.
          closeAndFlag();
        }
      } catch {
        /* ignore bad frames */
      }
    };
    es.onerror = () => {
      // EventSource will auto-retry by default; we want to give up on error
      // because backend closes the stream after run_completed and the browser
      // sees that as an error too.
      closeAndFlag();
    };
    return () => {
      es.close();
      sourceRef.current = null;
    };
  }, [runId]);

  return { state, io, events, closed };
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
