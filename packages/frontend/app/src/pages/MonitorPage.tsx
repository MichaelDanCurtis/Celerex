import { useAtomValue } from "jotai";
import { useEffect, useState } from "react";
import { AgentBuilderCanvas } from "../canvas/AgentBuilderCanvas";
import { StepDrawer } from "../canvas/StepDrawer";
import { activeRunIdAtom, activeMemberIdAtom, tokenAtom } from "../state/atoms";
import { useRunEvents } from "../api/sse";
import { getMember } from "../api/client";
import { Graph } from "../canvas/types";

export function MonitorPage() {
  const runId = useAtomValue(activeRunIdAtom);
  const memberId = useAtomValue(activeMemberIdAtom);
  const token = useAtomValue(tokenAtom);
  const { state: runState, io } = useRunEvents(runId);
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
      <div style={{ padding: 12, borderBottom: "1px solid var(--celerex-border)" }}>
        <label>Run: </label>
        <select data-testid="run-selector" value={runId ?? ""} onChange={() => {}}>
          <option value={runId ?? ""}>{runId ?? "Live"}</option>
        </select>
        <span style={{ marginLeft: 12, opacity: 0.6 }}>member: {memberId ?? "—"}</span>
      </div>
      <div style={{ flex: 1, display: "flex", position: "relative" }}>
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
    </div>
  );
}
