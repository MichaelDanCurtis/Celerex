import { useEffect, useState } from "react";
import { useSetAtom } from "jotai";
import { fetchFleet, type FleetRow } from "../api/client";
import { activeMemberIdAtom, activeViewAtom } from "../state/atoms";

export function FleetDashboard({ token }: { token: string }) {
  const [rows, setRows] = useState<FleetRow[]>([]);
  const setMember = useSetAtom(activeMemberIdAtom);
  const setView = useSetAtom(activeViewAtom);

  useEffect(() => {
    let cancelled = false;
    const tick = async () => {
      try {
        const r = await fetchFleet(token);
        if (!cancelled) setRows(r);
      } catch {
        // swallow — poll retries
      }
    };
    tick();
    const id = setInterval(tick, 5000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [token]);

  return (
    <div style={{ padding: 16, flex: 1, overflow: "auto" }}>
      <h2>Fleet</h2>
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
          {rows.map((r) => (
            <tr
              key={r.id}
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
