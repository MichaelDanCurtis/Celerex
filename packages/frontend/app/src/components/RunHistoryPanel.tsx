import { useCallback, useEffect, useState } from "react";
import { useAtom, useSetAtom } from "jotai";
import { listRuns, type ApiRunSummary } from "../api/client";
import { activeRunIdAtom, activeViewAtom } from "../state/atoms";
import { addToastAtom } from "../state/toastAtom";

export interface RunHistoryPanelProps {
  token: string;
  projectMemberId: string;
}

function truncateId(id: string): string {
  return id.length > 8 ? `${id.slice(0, 8)}…` : id;
}

function formatRelative(iso: string): string {
  try {
    const then = new Date(iso).getTime();
    const now = Date.now();
    const diff = Math.max(0, now - then);
    const sec = Math.floor(diff / 1000);
    if (sec < 60) return `${sec}s ago`;
    const min = Math.floor(sec / 60);
    if (min < 60) return `${min}m ago`;
    const hr = Math.floor(min / 60);
    if (hr < 24) return `${hr}h ago`;
    const days = Math.floor(hr / 24);
    if (days < 30) return `${days}d ago`;
    return new Date(iso).toLocaleDateString();
  } catch {
    return iso;
  }
}

function statusBadgeStyle(status: string): React.CSSProperties {
  const palette: Record<string, { bg: string; color: string }> = {
    succeeded: { bg: "#dcfce7", color: "#166534" },
    running: { bg: "#dbeafe", color: "#1e40af" },
    pending: { bg: "#fef3c7", color: "#92400e" },
    failed: { bg: "#fee2e2", color: "#991b1b" },
  };
  const c = palette[status.toLowerCase()] ?? { bg: "#e5e7eb", color: "#1f2937" };
  return {
    background: c.bg,
    color: c.color,
    padding: "2px 8px",
    borderRadius: 999,
    fontSize: 11,
    fontWeight: 500,
    textTransform: "lowercase",
    display: "inline-block",
  };
}

export function RunHistoryPanel({ token, projectMemberId }: RunHistoryPanelProps) {
  const [runs, setRuns] = useState<ApiRunSummary[]>([]);
  const [loading, setLoading] = useState(false);
  const setRun = useSetAtom(activeRunIdAtom);
  const [, setView] = useAtom(activeViewAtom);
  const addToast = useSetAtom(addToastAtom);

  const refresh = useCallback(async () => {
    if (!projectMemberId) return;
    setLoading(true);
    try {
      const rows = await listRuns(token, projectMemberId);
      setRuns(rows);
    } catch (e) {
      addToast({
        message: e instanceof Error ? e.message : "Failed to load runs",
        type: "error",
      });
    } finally {
      setLoading(false);
    }
  }, [token, projectMemberId, addToast]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const onRowClick = (run: ApiRunSummary) => {
    setRun(run.id);
    setView("monitor");
  };

  return (
    <div
      data-testid="run-history-panel"
      style={{
        padding: 16,
        borderTop: "1px solid var(--celerex-border)",
        background: "var(--celerex-bg-primary)",
        overflow: "auto",
      }}
    >
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginBottom: 12,
        }}
      >
        <h3 style={{ margin: 0, fontSize: 14, fontWeight: 600 }}>Run history</h3>
        <button
          data-testid="refresh-runs"
          onClick={() => void refresh()}
          style={{
            padding: "4px 10px",
            borderRadius: 6,
            border: "1px solid var(--celerex-border)",
            background: "transparent",
            color: "var(--celerex-text-secondary)",
            cursor: "pointer",
            fontSize: 12,
          }}
        >
          Refresh
        </button>
      </div>

      {loading ? (
        <div style={{ opacity: 0.6, fontSize: 13 }}>Loading…</div>
      ) : runs.length === 0 ? (
        <div style={{ opacity: 0.6, fontSize: 13 }} data-testid="run-history-empty">
          No runs yet.
        </div>
      ) : (
        <table
          data-testid="run-history-table"
          style={{ width: "100%", borderCollapse: "collapse" }}
        >
          <thead>
            <tr>
              <th style={{ textAlign: "left", padding: "6px 8px", fontSize: 12 }}>Run</th>
              <th style={{ textAlign: "left", padding: "6px 8px", fontSize: 12 }}>Status</th>
              <th style={{ textAlign: "left", padding: "6px 8px", fontSize: 12 }}>Trigger</th>
              <th style={{ textAlign: "left", padding: "6px 8px", fontSize: 12 }}>When</th>
            </tr>
          </thead>
          <tbody>
            {runs.map((r) => (
              <tr
                key={r.id}
                data-testid={`run-row-${r.id}`}
                onClick={() => onRowClick(r)}
                style={{
                  borderTop: "1px solid var(--celerex-border)",
                  cursor: "pointer",
                }}
              >
                <td
                  style={{
                    padding: "6px 8px",
                    fontSize: 13,
                    fontFamily: "monospace",
                  }}
                  title={r.id}
                >
                  {truncateId(r.id)}
                </td>
                <td style={{ padding: "6px 8px", fontSize: 13 }}>
                  <span style={statusBadgeStyle(r.status)}>{r.status}</span>
                </td>
                <td style={{ padding: "6px 8px", fontSize: 13 }}>{r.triggered_by}</td>
                <td
                  style={{ padding: "6px 8px", fontSize: 13 }}
                  title={r.created_at}
                >
                  {formatRelative(r.created_at)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
