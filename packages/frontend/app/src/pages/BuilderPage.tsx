import { useState } from "react";
import { useAtom, useSetAtom } from "jotai";
import { AgentBuilderCanvas } from "../canvas/AgentBuilderCanvas";
import { NodeConfigPanel } from "../canvas/NodeConfigPanel";
import { Graph, GraphNode } from "../canvas/types";
import { createMember, runMemberNow } from "../api/client";
import {
  activeMemberIdAtom,
  activeRunIdAtom,
  activeViewAtom,
} from "../state/atoms";

export function BuilderPage({
  projectId,
  token,
}: {
  projectId: string;
  token: string;
}) {
  const [graph, setGraph] = useState<Graph>({ nodes: [], edges: [] });
  const [selected, setSelected] = useState<GraphNode | null>(null);
  const [name, setName] = useState("New agent");
  const setMember = useSetAtom(activeMemberIdAtom);
  const setRun = useSetAtom(activeRunIdAtom);
  const [, setView] = useAtom(activeViewAtom);

  const onRunNow = async () => {
    const m = await createMember(token, {
      project_id: projectId,
      name,
      graph_json: graph,
    });
    setMember(m.id);
    const { run_id } = await runMemberNow(token, m.id);
    setRun(run_id);
    setView("monitor");
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", flex: 1 }}>
      <div
        style={{
          padding: 12,
          borderBottom: "1px solid var(--celerex-border)",
          display: "flex",
          gap: 8,
        }}
      >
        <input value={name} onChange={(e) => setName(e.target.value)} />
        <button data-testid="run-now" onClick={onRunNow}>
          Run now
        </button>
      </div>
      <div style={{ flex: 1, display: "flex" }}>
        <AgentBuilderCanvas graph={graph} mode="edit" onEdit={setGraph} />
        <NodeConfigPanel
          node={selected}
          onChange={(n) => {
            setGraph((prev) => ({
              ...prev,
              nodes: prev.nodes.map((x) => (x.id === n.id ? n : x)),
            }));
            setSelected(n);
          }}
        />
      </div>
    </div>
  );
}
