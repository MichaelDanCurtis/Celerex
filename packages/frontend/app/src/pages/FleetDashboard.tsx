import { useEffect, useState } from "react";
import { useSetAtom } from "jotai";
import { fetchFleet, type FleetRow } from "../api/client";
import { activeMemberIdAtom, activeViewAtom } from "../state/atoms";

export function FleetDashboard({ token }: { token: string }) {
  const [rows, setRows] = useState<FleetRow[]>([]);
  const [lastUpdated, setLastUpdated] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const setMember = useSetAtom(activeMemberIdAtom);
  const setView = useSetAtom(activeViewAtom);

  useEffect(() => {
    let cancelled = false;
    const tick = async () => {
      try {
        const r = await fetchFleet(token);
        if (!cancelled) {
          setRows(r);
          setLastUpdated(Date.now());
          setError(null);
        }
      } catch (e) {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : "Fleet refresh failed");
        }
        // swallow — poll retries
      }
    };
    void tick();
    const id = setInterval(tick, 5000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [token]);

  return (
    <div style={{ padding: 16, flex: 1, overflow: "auto" }}>
      <div
        style={{
          display: "flex",
          alignItems: "baseline",
          justifyContent: "space-between",
          marginBottom: 8,
        }}
      >
        <h2 style={{ margin: 0 }}>Fleet</h2>
        <span
          data-testid="fleet-last-updated"
          style={{ fontSize: 12, color: "var(--celerex-text-secondary)" }}
        >
          {error
            ? `Refresh error: ${error}`
            : lastUpdated
              ? `Last updated: ${formatTime(lastUpdated)}`
              : "Loading…"}
        </span>
      </div>
      <table
        data-testid="fleet-table"
        style={{ width: "100%", borderCollapse: "collapse" }}
      >
        <thead>
          <tr>
            <th style={{ textAlign: "left" }}>Name</th>
            <th style={{ textAlign: "left" }}>Last status</th>
            <th style={{ textAlign: "left" }}>Next fire</th>
            <th style={{ textAlign: "left" }}>Running?</th>
          </tr>
        </thead>
        <tbody>
          {rows.length === 0 && (
            <tr>
              <td
                colSpan={4}
                style={{
                  padding: "12px 4px",
                  color: "var(--celerex-text-secondary)",
                  fontStyle: "italic",
                }}
              >
                {lastUpdated ? "No agents yet." : "Loading fleet…"}
              </td>
            </tr>
          )}
          {rows.map((r) => (
            <tr
              key={r.id}
              data-testid={`fleet-row-${r.id}`}
              onClick={() => {
                setMember(r.id);
                setView("monitor");
              }}
              style={{ cursor: "pointer" }}
            >
              <td>{r.name}</td>
              <td>{r.last_status ?? "—"}</td>
              <td>{r.next_fire_at ?? "—"}</td>
              <td>{r.running ? "● live" : "—"}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function formatTime(ts: number): string {
  const d = new Date(ts);
  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}
