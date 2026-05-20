import { useAtomValue } from "jotai";
import { useEffect, useState } from "react";
import { AgentBuilderCanvas } from "../canvas/AgentBuilderCanvas";
import { StepDrawer } from "../canvas/StepDrawer";
import { activeRunIdAtom, activeMemberIdAtom, tokenAtom } from "../state/atoms";
import { useRunEvents, type RunEventLogEntry } from "../api/sse";
import { getMember } from "../api/client";
import { Graph } from "../canvas/types";

export function MonitorPage() {
  const runId = useAtomValue(activeRunIdAtom);
  const memberId = useAtomValue(activeMemberIdAtom);
  const token = useAtomValue(tokenAtom);
  const { state: runState, io, events, closed } = useRunEvents(runId);
  const [graph, setGraph] = useState<Graph>({ nodes: [], edges: [] });
  const [selected, setSelected] = useState<string | null>(null);

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

  return (
    <div style={{ display: "flex", flexDirection: "column", flex: 1, position: "relative" }}>
      <div
        style={{
          padding: 12,
          borderBottom: "1px solid var(--celerex-border)",
          display: "flex",
          alignItems: "center",
          gap: 12,
        }}
      >
        <label>Run: </label>
        <select data-testid="run-selector" value={runId ?? ""} onChange={() => {}}>
          <option value={runId ?? ""}>{runId ?? "Live"}</option>
        </select>
        <span style={{ opacity: 0.6 }}>member: {memberId ?? "—"}</span>
        <span
          data-testid="run-status"
          style={{
            marginLeft: "auto",
            fontSize: 12,
            color: runId
              ? closed
                ? "var(--celerex-text-secondary)"
                : "var(--celerex-accent)"
              : "var(--celerex-text-secondary)",
          }}
        >
          {!runId
            ? "No active run"
            : closed
              ? "Run completed"
              : `Live · ${events.length} event${events.length === 1 ? "" : "s"}`}
        </span>
      </div>
      <div style={{ flex: 1, display: "flex", position: "relative", minHeight: 0 }}>
        <div style={{ flex: 1, position: "relative", display: "flex" }}>
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
        <RunEventsLog events={events} closed={closed} hasRun={!!runId} />
      </div>
    </div>
  );
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
        borderLeft: "1px solid var(--celerex-border)",
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
        background: "var(--celerex-bg-secondary, transparent)",
      }}
    >
      <div
        style={{
          padding: "8px 12px",
          borderBottom: "1px solid var(--celerex-border)",
          fontSize: 12,
          fontWeight: 600,
          textTransform: "uppercase",
          letterSpacing: 0.5,
          color: "var(--celerex-text-secondary)",
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
            Run finished with no events.
          </div>
        )}
        <table
          data-testid="run-events-table"
          style={{ width: "100%", borderCollapse: "collapse", fontSize: 12 }}
        >
          <tbody>
            {events.map((entry) => (
              <tr key={entry.seq} style={{ verticalAlign: "top" }}>
                <td style={{ padding: "4px 6px", whiteSpace: "nowrap", opacity: 0.6 }}>
                  {formatTime(entry.ts)}
                </td>
                <td style={{ padding: "4px 6px" }}>
                  <div style={{ fontWeight: 500, color: eventColor(entry.ev.event) }}>
                    {eventLabel(entry.ev.event)}
                  </div>
                  {entry.ev.step_id && (
                    <div style={{ opacity: 0.75 }}>{entry.ev.step_id}</div>
                  )}
                  {entry.ev.error && (
                    <div
                      style={{
                        marginTop: 2,
                        color: "var(--celerex-danger, #c0392b)",
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
      return "var(--celerex-success, #16a34a)";
    case "run_step_failed":
      return "var(--celerex-danger, #c0392b)";
    case "run_step_started":
      return "var(--celerex-accent)";
    default:
      return "var(--celerex-text)";
  }
}

function formatTime(ts: number): string {
  const d = new Date(ts);
  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}
