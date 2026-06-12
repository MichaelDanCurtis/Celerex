import { useCallback, useEffect, useMemo, useState } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { tokenAtom, activeViewAtom } from "../state/atoms";
import {
  listRecentRuns,
  listRunSteps,
  type ApiRunActivity,
  type ApiRunStep,
} from "../api/client";

const STATUS_STYLE: Record<string, { bg: string; fg: string; label: string }> = {
  succeeded: { bg: "rgba(34,197,94,0.16)", fg: "#22c55e", label: "Succeeded" },
  failed: { bg: "rgba(239,68,68,0.16)", fg: "#f87171", label: "Failed" },
  running: { bg: "rgba(59,130,246,0.16)", fg: "#60a5fa", label: "Running" },
  awaiting_review: { bg: "rgba(251,191,36,0.16)", fg: "#fbbf24", label: "Awaiting review" },
  pending: { bg: "rgba(148,163,184,0.16)", fg: "#94a3b8", label: "Pending" },
};

function statusStyle(s: string) {
  return STATUS_STYLE[s] ?? { bg: "rgba(148,163,184,0.16)", fg: "#94a3b8", label: s };
}

function timeAgo(iso: string): string {
  const secs = Math.max(0, Math.round((Date.now() - new Date(iso).getTime()) / 1000));
  if (secs < 60) return "just now";
  const mins = Math.round(secs / 60);
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.round(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  return `${Math.round(hrs / 24)}d ago`;
}

function StatusPill({ status }: { status: string }) {
  const s = statusStyle(status);
  return (
    <span
      style={{
        display: "inline-block",
        padding: "2px 10px",
        borderRadius: 999,
        background: s.bg,
        color: s.fg,
        fontSize: 11.5,
        fontWeight: 700,
        whiteSpace: "nowrap",
      }}
    >
      {s.label}
    </span>
  );
}

/**
 * Activity — the engagement-wide run feed. Every flow run across the workspace,
 * newest first, with drill-down into the per-step timeline. Paused runs are
 * surfaced with a jump to Approvals.
 */
export function ActivityPage() {
  const token = useAtomValue(tokenAtom);
  const setView = useSetAtom(activeViewAtom);
  const [runs, setRuns] = useState<ApiRunActivity[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [openRun, setOpenRun] = useState<string | null>(null);
  const [steps, setSteps] = useState<ApiRunStep[] | null>(null);
  const [stepsLoading, setStepsLoading] = useState(false);

  const refresh = useCallback(async () => {
    if (!token) return;
    try {
      setRuns(await listRecentRuns(token, 100));
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoaded(true);
    }
  }, [token]);

  useEffect(() => {
    void refresh();
    const id = setInterval(() => void refresh(), 5000);
    return () => clearInterval(id);
  }, [refresh]);

  async function toggleRun(runId: string) {
    if (openRun === runId) {
      setOpenRun(null);
      setSteps(null);
      return;
    }
    setOpenRun(runId);
    setSteps(null);
    setStepsLoading(true);
    try {
      setSteps(await listRunSteps(token, runId));
    } catch {
      setSteps([]);
    } finally {
      setStepsLoading(false);
    }
  }

  const counts = useMemo(() => {
    const c: Record<string, number> = {};
    for (const r of runs) c[r.status] = (c[r.status] ?? 0) + 1;
    return c;
  }, [runs]);

  return (
    <div style={{ flex: 1, height: "100%", overflow: "auto", background: "var(--celerex-bg-primary)" }}>
      <div style={{ maxWidth: 960, margin: "0 auto", padding: "32px 24px" }}>
        <h1 style={{ margin: 0, fontSize: 22, fontWeight: 700, color: "var(--celerex-text-primary)" }}>
          Activity
        </h1>
        <p style={{ margin: "4px 0 20px", fontSize: 14, color: "var(--celerex-text-secondary)" }}>
          Every flow run across your workspace. Click a run to see its step-by-step timeline.
        </p>

        {/* Summary chips */}
        <div style={{ display: "flex", gap: 10, flexWrap: "wrap", marginBottom: 20 }}>
          <SummaryChip label="Total" value={runs.length} />
          {["awaiting_review", "running", "succeeded", "failed"].map((s) =>
            counts[s] ? (
              <button
                key={s}
                onClick={() => s === "awaiting_review" && setView("task-queue")}
                style={{
                  border: "none",
                  background: "transparent",
                  padding: 0,
                  cursor: s === "awaiting_review" ? "pointer" : "default",
                }}
                title={s === "awaiting_review" ? "Go to Approvals" : undefined}
              >
                <SummaryChip label={statusStyle(s).label} value={counts[s]} accent={statusStyle(s).fg} />
              </button>
            ) : null,
          )}
        </div>

        {error && (
          <div
            style={{
              marginBottom: 16,
              padding: "10px 14px",
              borderRadius: 8,
              border: "1px solid #7f1d1d",
              background: "rgba(127,29,29,0.18)",
              color: "#fca5a5",
              fontSize: 13,
            }}
          >
            {error}
          </div>
        )}

        {loaded && runs.length === 0 ? (
          <div
            style={{
              padding: "48px 24px",
              textAlign: "center",
              color: "var(--celerex-text-secondary)",
              border: "1px dashed var(--celerex-border)",
              borderRadius: 12,
            }}
          >
            <div style={{ fontSize: 30, marginBottom: 8 }}>📡</div>
            <div style={{ fontWeight: 600, color: "var(--celerex-text-primary)" }}>No runs yet</div>
            <div style={{ fontSize: 13, marginTop: 4 }}>Run a flow and it’ll show up here.</div>
          </div>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
            {runs.map((run) => {
              const open = openRun === run.id;
              const paused = run.status === "awaiting_review";
              return (
                <div
                  key={run.id}
                  style={{
                    border: `1px solid ${open ? "var(--celerex-accent)" : "var(--celerex-border)"}`,
                    borderRadius: 10,
                    background: "var(--celerex-bg-secondary, #13161f)",
                    overflow: "hidden",
                  }}
                >
                  <button
                    onClick={() => void toggleRun(run.id)}
                    style={{
                      width: "100%",
                      display: "flex",
                      alignItems: "center",
                      gap: 12,
                      padding: "12px 16px",
                      background: "transparent",
                      border: "none",
                      cursor: "pointer",
                      textAlign: "left",
                      color: "var(--celerex-text-primary)",
                    }}
                  >
                    <StatusPill status={run.status} />
                    <span style={{ flex: 1, minWidth: 0, fontSize: 14, fontWeight: 600 }}>
                      {run.flow_name ?? "Flow"}
                    </span>
                    <span style={{ fontSize: 12, color: "var(--celerex-text-secondary)" }}>
                      {run.triggered_by}
                    </span>
                    <span style={{ fontSize: 12, color: "var(--celerex-text-secondary)" }}>
                      {timeAgo(run.created_at)}
                    </span>
                    <span style={{ fontSize: 12, color: "var(--celerex-text-secondary)" }}>{open ? "▲" : "▼"}</span>
                  </button>

                  {open && (
                    <div style={{ padding: "0 16px 14px" }}>
                      {paused && (
                        <button
                          onClick={() => setView("task-queue")}
                          style={{
                            marginBottom: 10,
                            padding: "6px 12px",
                            borderRadius: 7,
                            border: "1px solid #fbbf24",
                            background: "rgba(251,191,36,0.1)",
                            color: "#fbbf24",
                            fontSize: 12.5,
                            fontWeight: 600,
                            cursor: "pointer",
                          }}
                        >
                          → Review this draft in Approvals
                        </button>
                      )}
                      {stepsLoading && (
                        <div style={{ fontSize: 13, color: "var(--celerex-text-secondary)" }}>Loading steps…</div>
                      )}
                      {steps && steps.length === 0 && !stepsLoading && (
                        <div style={{ fontSize: 13, color: "var(--celerex-text-secondary)" }}>
                          No step records for this run.
                        </div>
                      )}
                      {steps && steps.length > 0 && (
                        <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                          {steps.map((s, i) => (
                            <StepRow key={`${s.step_id}-${i}`} step={s} />
                          ))}
                        </div>
                      )}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}

function SummaryChip({ label, value, accent }: { label: string; value: number; accent?: string }) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "baseline",
        gap: 6,
        padding: "8px 14px",
        borderRadius: 10,
        border: "1px solid var(--celerex-border)",
        background: "var(--celerex-bg-secondary, #13161f)",
      }}
    >
      <span style={{ fontSize: 18, fontWeight: 800, color: accent ?? "var(--celerex-text-primary)" }}>{value}</span>
      <span style={{ fontSize: 12, color: "var(--celerex-text-secondary)" }}>{label}</span>
    </div>
  );
}

function StepRow({ step }: { step: ApiRunStep }) {
  const s = statusStyle(step.status);
  const out =
    step.error ??
    (typeof step.outputs === "string"
      ? step.outputs
      : step.outputs
        ? JSON.stringify(step.outputs)
        : "");
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 10,
        padding: "8px 12px",
        borderRadius: 8,
        border: "1px solid var(--celerex-border)",
        background: "var(--celerex-bg-primary)",
      }}
    >
      <span style={{ width: 8, height: 8, borderRadius: 999, background: s.fg, flexShrink: 0 }} />
      <span style={{ fontSize: 13, fontWeight: 600, color: "var(--celerex-text-primary)", minWidth: 90 }}>
        {step.step_id}
      </span>
      <span style={{ fontSize: 11.5, color: s.fg, fontWeight: 600, minWidth: 100 }}>{s.label}</span>
      <span
        style={{
          flex: 1,
          minWidth: 0,
          fontSize: 12,
          color: step.error ? "#f87171" : "var(--celerex-text-secondary)",
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
          fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
        }}
        title={out}
      >
        {out}
      </span>
    </div>
  );
}
