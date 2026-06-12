import { useAtomValue } from "jotai";
import { useEffect, useMemo, useState } from "react";
import { AgentBuilderCanvas } from "../canvas/AgentBuilderCanvas";
import { StepDrawer } from "../canvas/StepDrawer";
import { activeRunIdAtom, activeMemberIdAtom, tokenAtom } from "../state/atoms";
import { useRunEvents, type RunEventLogEntry, type ServerEvent } from "../api/sse";
import {
  getMember,
  listRuns,
  listRunSteps,
  type ApiRunStep,
  type ApiRunSummary,
} from "../api/client";
import { Graph, RunState } from "../canvas/types";
import type { StepIO } from "../api/sse";

export function MonitorPage() {
  const liveRunId = useAtomValue(activeRunIdAtom);
  const memberId = useAtomValue(activeMemberIdAtom);
  const token = useAtomValue(tokenAtom);

  const [runs, setRuns] = useState<ApiRunSummary[]>([]);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(liveRunId);
  const [graph, setGraph] = useState<Graph>({ nodes: [], edges: [] });
  const [selected, setSelected] = useState<string | null>(null);
  const [histSteps, setHistSteps] = useState<ApiRunStep[] | null>(null);

  // Follow the live run when a new one starts.
  useEffect(() => {
    setSelectedRunId(liveRunId);
  }, [liveRunId]);

  // The SSE stream only runs for the CURRENT live run; history is fetched.
  const streaming = selectedRunId !== null && selectedRunId === liveRunId;
  const { state: sseState, io: sseIo, events: sseEvents, closed } = useRunEvents(
    streaming ? selectedRunId : null,
  );

  // Load the member's flow graph for the canvas.
  useEffect(() => {
    if (!memberId) return;
    let cancelled = false;
    getMember(token, memberId)
      .then((m) => {
        if (cancelled) return;
        setGraph({
          nodes: m.graph_json.nodes.map((n) => ({
            id: n.id,
            type: n.type as Graph["nodes"][number]["type"],
            config: n.config,
            position: (n as { position?: { x: number; y: number } }).position,
          })),
          edges: m.graph_json.edges,
        });
      })
      .catch(() => {
        /* swallow; monitor stays empty */
      });
    return () => {
      cancelled = true;
    };
  }, [memberId, token]);

  // Run history for the selector.
  useEffect(() => {
    if (!memberId || !token) return;
    let cancelled = false;
    listRuns(token, memberId, 25)
      .then((rows) => {
        if (!cancelled) setRuns(rows);
      })
      .catch(() => {
        /* selector just stays sparse */
      });
    return () => {
      cancelled = true;
    };
  }, [memberId, token, liveRunId, closed]);

  // Historical runs: load persisted step records instead of streaming.
  useEffect(() => {
    if (!selectedRunId || streaming || !token) {
      setHistSteps(null);
      return;
    }
    let cancelled = false;
    listRunSteps(token, selectedRunId)
      .then((rows) => {
        if (!cancelled) setHistSteps(rows);
      })
      .catch(() => {
        if (!cancelled) setHistSteps([]);
      });
    return () => {
      cancelled = true;
    };
  }, [selectedRunId, streaming, token]);

  // Merge live vs historical into one view-model for the canvas + drawer + log.
  const { runState, io, events } = useMemo(() => {
    if (streaming || !histSteps) {
      return { runState: sseState, io: sseIo, events: sseEvents };
    }
    const steps: RunState["steps"] = {};
    const histIo: Record<string, StepIO> = {};
    const histEvents: RunEventLogEntry[] = histSteps.map((s, i) => {
      steps[s.step_id] =
        s.status === "succeeded" ? "succeeded" : s.status === "failed" ? "failed" : "idle";
      histIo[s.step_id] = { outputs: s.outputs ?? undefined, error: s.error ?? undefined };
      const ev: ServerEvent = {
        event: s.status === "failed" ? "run_step_failed" : "run_step_succeeded",
        run_id: selectedRunId ?? "",
        step_id: s.step_id,
        error: s.error ?? undefined,
      };
      return { seq: i + 1, ts: s.completed_at ? Date.parse(s.completed_at) : Date.now(), ev };
    });
    return { runState: { steps }, io: histIo, events: histEvents };
  }, [streaming, histSteps, sseState, sseIo, sseEvents, selectedRunId]);

  const selectedSummary = runs.find((r) => r.id === selectedRunId);
  const statusText = !selectedRunId
    ? "No run selected — start one from the Builder."
    : streaming
      ? closed
        ? "Run completed"
        : `Live · ${events.length} event${events.length === 1 ? "" : "s"}`
      : `History · ${selectedSummary?.status ?? "…"}`;

  return (
    <div style={{ display: "flex", flexDirection: "column", flex: 1, position: "relative", minHeight: 0 }}>
      <div
        style={{
          padding: "10px 12px",
          borderBottom: "1px solid #1d2230",
          background: "#0c0e15",
          display: "flex",
          alignItems: "center",
          gap: 12,
          color: "#aab2c6",
          fontSize: 13,
        }}
      >
        <label style={{ fontWeight: 600 }}>Run</label>
        <select
          data-testid="run-selector"
          value={selectedRunId ?? ""}
          onChange={(e) => setSelectedRunId(e.target.value || null)}
          style={{
            padding: "6px 8px",
            borderRadius: 7,
            border: "1px solid #232838",
            background: "#11141d",
            color: "#e6e9f0",
            fontSize: 12.5,
            maxWidth: 340,
          }}
        >
          {!runs.some((r) => r.id === selectedRunId) && selectedRunId && (
            <option value={selectedRunId}>{selectedRunId.slice(0, 8)} · live</option>
          )}
          {runs.length === 0 && !selectedRunId && <option value="">—</option>}
          {runs.map((r) => (
            <option key={r.id} value={r.id}>
              {r.id.slice(0, 8)} · {r.status} · {formatWhen(r.created_at)}
            </option>
          ))}
        </select>
        <StatusPill status={streaming ? (closed ? "done" : "live") : selectedSummary?.status} />
        <span
          data-testid="run-status"
          style={{ marginLeft: "auto", fontSize: 12, color: streaming && !closed ? "#60a5fa" : "#6b7488" }}
        >
          {statusText}
        </span>
      </div>
      <div style={{ flex: 1, display: "flex", position: "relative", minHeight: 0 }}>
        <div style={{ flex: 1, position: "relative", display: "flex", minWidth: 0 }}>
          <AgentBuilderCanvas
            graph={graph}
            mode="monitor"
            runState={runState}
            onNodeClick={(id) => setSelected(id)}
          />
          {selected && (
            <StepDrawer
              stepId={selected}
              outputs={io[selected]?.outputs}
              error={io[selected]?.error}
              onClose={() => setSelected(null)}
            />
          )}
        </div>
        <RunEventsLog events={events} closed={streaming ? closed : true} hasRun={!!selectedRunId} />
      </div>
    </div>
  );
}

function StatusPill({ status }: { status?: string }) {
  if (!status) return null;
  const color =
    status === "live"
      ? "#60a5fa"
      : status === "succeeded" || status === "done"
        ? "#34d399"
        : status === "failed"
          ? "#f87171"
          : "#9aa3b8";
  return (
    <span
      style={{
        padding: "2px 10px",
        borderRadius: 999,
        fontSize: 11,
        fontWeight: 700,
        textTransform: "uppercase",
        letterSpacing: 0.6,
        color,
        border: `1px solid ${color}55`,
        background: `${color}18`,
      }}
    >
      {status}
    </span>
  );
}

function formatWhen(iso: string): string {
  const d = new Date(iso);
  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${pad(d.getMonth() + 1)}/${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function RunEventsLog({
  events,
  closed,
  hasRun,
}: {
  events: RunEventLogEntry[];
  closed: boolean;
  hasRun: boolean;
}) {
  return (
    <aside
      data-testid="run-events-log"
      style={{
        width: 320,
        flexShrink: 0,
        borderLeft: "1px solid #1d2230",
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
        background: "#0c0e15",
        color: "#cbd2e0",
      }}
    >
      <div
        style={{
          padding: "8px 12px",
          borderBottom: "1px solid #1d2230",
          fontSize: 12,
          fontWeight: 600,
          textTransform: "uppercase",
          letterSpacing: 0.5,
          color: "#6b7488",
        }}
      >
        Step events
      </div>
      <div style={{ flex: 1, overflow: "auto", padding: 8 }}>
        {!hasRun && (
          <div style={{ fontSize: 12, opacity: 0.6, padding: 8 }}>
            Start a run from the Builder to see live step events here.
          </div>
        )}
        {hasRun && events.length === 0 && !closed && (
          <div style={{ fontSize: 12, opacity: 0.6, padding: 8 }}>
            Waiting for first event…
          </div>
        )}
        {hasRun && events.length === 0 && closed && (
          <div style={{ fontSize: 12, opacity: 0.6, padding: 8 }}>
            No step records for this run.
          </div>
        )}
        <table
          data-testid="run-events-table"
          style={{ width: "100%", borderCollapse: "collapse", fontSize: 12 }}
        >
          <tbody>
            {events.map((entry) => (
              <tr key={entry.seq} style={{ verticalAlign: "top" }}>
                <td style={{ padding: "4px 6px", whiteSpace: "nowrap", opacity: 0.5 }}>
                  {formatTime(entry.ts)}
                </td>
                <td style={{ padding: "4px 6px" }}>
                  <div style={{ fontWeight: 500, color: eventColor(entry.ev.event) }}>
                    {eventLabel(entry.ev.event)}
                  </div>
                  {entry.ev.step_id && (
                    <div style={{ opacity: 0.7 }}>{entry.ev.step_id}</div>
                  )}
                  {entry.ev.error && (
                    <div
                      style={{
                        marginTop: 2,
                        color: "#f87171",
                        whiteSpace: "pre-wrap",
                        wordBreak: "break-word",
                      }}
                    >
                      {entry.ev.error}
                    </div>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </aside>
  );
}

function eventLabel(ev: string): string {
  switch (ev) {
    case "run_step_started":
      return "step started";
    case "run_step_succeeded":
      return "step succeeded";
    case "run_step_failed":
      return "step failed";
    case "run_completed":
      return "run completed";
    default:
      return ev;
  }
}

function eventColor(ev: string): string {
  switch (ev) {
    case "run_step_succeeded":
    case "run_completed":
      return "#34d399";
    case "run_step_failed":
      return "#f87171";
    case "run_step_started":
      return "#60a5fa";
    default:
      return "#cbd2e0";
  }
}

function formatTime(ts: number): string {
  const d = new Date(ts);
  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}
