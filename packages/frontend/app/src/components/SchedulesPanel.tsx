import { useCallback, useEffect, useState } from "react";
import { useSetAtom } from "jotai";
import {
  createSchedule,
  deleteSchedule,
  listSchedules,
  type ApiSchedule,
} from "../api/client";
import { addToastAtom } from "../state/toastAtom";

const TIMEZONES = [
  "UTC",
  "America/New_York",
  "America/Los_Angeles",
  "Europe/London",
  "Asia/Tokyo",
];

function formatFire(iso: string | null | undefined): string {
  if (!iso) return "—";
  try {
    return new Date(iso).toLocaleString();
  } catch {
    return iso;
  }
}

export interface SchedulesPanelProps {
  token: string;
  projectMemberId: string;
}

export function SchedulesPanel({ token, projectMemberId }: SchedulesPanelProps) {
  const [schedules, setSchedules] = useState<ApiSchedule[]>([]);
  const [loading, setLoading] = useState(false);
  const [cronExpr, setCronExpr] = useState("");
  const [timezone, setTimezone] = useState<string>(TIMEZONES[0]);
  const [enabled, setEnabled] = useState(true);
  const [submitting, setSubmitting] = useState(false);
  const addToast = useSetAtom(addToastAtom);

  const refresh = useCallback(async () => {
    if (!projectMemberId) return;
    setLoading(true);
    try {
      const rows = await listSchedules(token, projectMemberId);
      setSchedules(rows);
    } catch (e) {
      addToast({
        message: e instanceof Error ? e.message : "Failed to load schedules",
        type: "error",
      });
    } finally {
      setLoading(false);
    }
  }, [token, projectMemberId, addToast]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const handleAdd = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!cronExpr.trim()) {
      addToast({ message: "Cron expression required", type: "error" });
      return;
    }
    setSubmitting(true);
    try {
      await createSchedule(token, {
        project_member_id: projectMemberId,
        cron_expr: cronExpr.trim(),
        timezone,
        enabled,
      });
      addToast({ message: "Schedule created", type: "success" });
      setCronExpr("");
      setEnabled(true);
      await refresh();
    } catch (err) {
      addToast({
        message: err instanceof Error ? err.message : "Failed to create schedule",
        type: "error",
      });
    } finally {
      setSubmitting(false);
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await deleteSchedule(token, id);
      addToast({ message: "Schedule deleted", type: "success" });
      await refresh();
    } catch (err) {
      addToast({
        message: err instanceof Error ? err.message : "Failed to delete schedule",
        type: "error",
      });
    }
  };

  return (
    <div
      data-testid="schedules-panel"
      style={{
        padding: 16,
        borderTop: "1px solid var(--celerex-border)",
        background: "var(--celerex-bg-primary)",
        overflow: "auto",
      }}
    >
      <h3 style={{ margin: "0 0 12px", fontSize: 14, fontWeight: 600 }}>
        Schedules
      </h3>

      {loading ? (
        <div style={{ opacity: 0.6, fontSize: 13 }}>Loading…</div>
      ) : schedules.length === 0 ? (
        <div style={{ opacity: 0.6, fontSize: 13, marginBottom: 12 }}>
          No schedules yet.
        </div>
      ) : (
        <table
          data-testid="schedules-table"
          style={{ width: "100%", borderCollapse: "collapse", marginBottom: 12 }}
        >
          <thead>
            <tr>
              <th style={{ textAlign: "left", padding: "6px 8px", fontSize: 12 }}>
                Cron
              </th>
              <th style={{ textAlign: "left", padding: "6px 8px", fontSize: 12 }}>
                Timezone
              </th>
              <th style={{ textAlign: "left", padding: "6px 8px", fontSize: 12 }}>
                Next fire
              </th>
              <th style={{ textAlign: "left", padding: "6px 8px", fontSize: 12 }}>
                Enabled
              </th>
              <th style={{ textAlign: "left", padding: "6px 8px", fontSize: 12 }}>
                Actions
              </th>
            </tr>
          </thead>
          <tbody>
            {schedules.map((s) => (
              <tr
                key={s.id}
                style={{ borderTop: "1px solid var(--celerex-border)" }}
              >
                <td style={{ padding: "6px 8px", fontSize: 13, fontFamily: "monospace" }}>
                  {s.cron_expr}
                </td>
                <td style={{ padding: "6px 8px", fontSize: 13 }}>{s.timezone}</td>
                <td style={{ padding: "6px 8px", fontSize: 13 }}>
                  {formatFire(s.next_fire_at)}
                </td>
                <td style={{ padding: "6px 8px", fontSize: 13 }}>
                  <input type="checkbox" checked={s.enabled} readOnly />
                </td>
                <td style={{ padding: "6px 8px" }}>
                  <button
                    data-testid={`delete-schedule-${s.id}`}
                    onClick={() => void handleDelete(s.id)}
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
                    Delete
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      <form
        data-testid="add-schedule-form"
        onSubmit={(e) => void handleAdd(e)}
        style={{
          display: "flex",
          gap: 8,
          alignItems: "center",
          flexWrap: "wrap",
          padding: 10,
          borderRadius: 6,
          border: "1px solid var(--celerex-border)",
        }}
      >
        <input
          data-testid="schedule-cron-input"
          placeholder="cron, e.g. 0 9 * * *"
          value={cronExpr}
          onChange={(e) => setCronExpr(e.target.value)}
          style={{
            flex: "1 1 200px",
            padding: "6px 8px",
            borderRadius: 6,
            border: "1px solid var(--celerex-border)",
            fontSize: 13,
            fontFamily: "monospace",
          }}
        />
        <select
          data-testid="schedule-tz-select"
          value={timezone}
          onChange={(e) => setTimezone(e.target.value)}
          style={{
            padding: "6px 8px",
            borderRadius: 6,
            border: "1px solid var(--celerex-border)",
            fontSize: 13,
          }}
        >
          {TIMEZONES.map((tz) => (
            <option key={tz} value={tz}>
              {tz}
            </option>
          ))}
        </select>
        <label style={{ display: "inline-flex", alignItems: "center", gap: 4, fontSize: 13 }}>
          <input
            data-testid="schedule-enabled-checkbox"
            type="checkbox"
            checked={enabled}
            onChange={(e) => setEnabled(e.target.checked)}
          />
          Enabled
        </label>
        <button
          data-testid="add-schedule-submit"
          type="submit"
          disabled={submitting}
          style={{
            padding: "6px 12px",
            borderRadius: 6,
            border: "none",
            background: "var(--celerex-accent)",
            color: "#fff",
            cursor: submitting ? "not-allowed" : "pointer",
            fontSize: 13,
            fontWeight: 500,
            opacity: submitting ? 0.6 : 1,
          }}
        >
          {submitting ? "Adding…" : "Add schedule"}
        </button>
      </form>
    </div>
  );
}
