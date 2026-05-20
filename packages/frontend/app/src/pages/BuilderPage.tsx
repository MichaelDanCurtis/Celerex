import { useCallback, useEffect, useState } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { AgentBuilderCanvas } from "../canvas/AgentBuilderCanvas";
import { NodeConfigPanel } from "../canvas/NodeConfigPanel";
import { Graph, GraphNode } from "../canvas/types";
import {
  createMember,
  getMember,
  runMemberNow,
  updateMember,
  type ApiProjectMember,
} from "../api/client";
import {
  activeMemberIdAtom,
  activeRunIdAtom,
  activeViewAtom,
} from "../state/atoms";
import { addToastAtom } from "../state/toastAtom";
import { SchedulesPanel } from "../components/SchedulesPanel";

type RightTab = "config" | "schedules";

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
  const setMemberId = useSetAtom(activeMemberIdAtom);
  const setRun = useSetAtom(activeRunIdAtom);
  const [, setView] = useAtom(activeViewAtom);
  const addToast = useSetAtom(addToastAtom);
  const activeMemberId = useAtomValue(activeMemberIdAtom);
  const [providerConfig, setProviderConfig] = useState<Record<string, unknown>>({});
  const [tab, setTab] = useState<RightTab>("config");
  const [saving, setSaving] = useState(false);
  const [running, setRunning] = useState(false);

  // Hydrate canvas when an existing member is selected.
  useEffect(() => {
    if (!activeMemberId) return;
    let cancelled = false;
    getMember(token, activeMemberId)
      .then((m: ApiProjectMember) => {
        if (cancelled) return;
        setName(m.name);
        setGraph({
          nodes: m.graph_json.nodes.map((n) => ({
            id: n.id,
            type: n.type as Graph["nodes"][number]["type"],
            config: n.config,
          })),
          edges: m.graph_json.edges,
        });
        setProviderConfig(m.provider_config_json ?? {});
      })
      .catch((err) => {
        addToast({
          message:
            err instanceof Error ? err.message : "Failed to load agent",
          type: "error",
        });
      });
    return () => {
      cancelled = true;
    };
  }, [activeMemberId, token, addToast]);

  const handleSave = useCallback(async () => {
    if (!projectId && !activeMemberId) {
      addToast({ message: "No project selected", type: "error" });
      return;
    }
    setSaving(true);
    try {
      if (activeMemberId) {
        const m = await updateMember(token, activeMemberId, {
          name,
          graph_json: graph,
          provider_config_json: providerConfig,
        });
        setMemberId(m.id);
        addToast({ message: "Saved", type: "success" });
      } else {
        const m = await createMember(token, {
          project_id: projectId,
          name,
          graph_json: graph,
          provider_config_json: providerConfig,
        });
        setMemberId(m.id);
        addToast({ message: "Saved", type: "success" });
      }
    } catch (err) {
      addToast({
        message: err instanceof Error ? err.message : "Save failed",
        type: "error",
      });
    } finally {
      setSaving(false);
    }
  }, [
    projectId,
    activeMemberId,
    token,
    name,
    graph,
    providerConfig,
    setMemberId,
    addToast,
  ]);

  const onRunNow = useCallback(async () => {
    setRunning(true);
    try {
      let id = activeMemberId;
      if (!id) {
        const m = await createMember(token, {
          project_id: projectId,
          name,
          graph_json: graph,
          provider_config_json: providerConfig,
        });
        setMemberId(m.id);
        id = m.id;
      }
      const { run_id } = await runMemberNow(token, id!);
      setRun(run_id);
      addToast({ message: `Started run: ${run_id}`, type: "success" });
      setView("monitor");
    } catch (err) {
      addToast({
        message: err instanceof Error ? err.message : "Run failed",
        type: "error",
      });
    } finally {
      setRunning(false);
    }
  }, [
    activeMemberId,
    token,
    projectId,
    name,
    graph,
    providerConfig,
    setMemberId,
    setRun,
    setView,
    addToast,
  ]);

  const toolbarButtonStyle = (active = false) =>
    ({
      padding: "6px 12px",
      borderRadius: 6,
      border: "1px solid var(--celerex-border)",
      background: active ? "var(--celerex-accent)" : "transparent",
      color: active ? "#fff" : "var(--celerex-text-secondary)",
      cursor: "pointer",
      fontSize: 13,
      fontWeight: 500,
    }) as const;

  return (
    <div style={{ display: "flex", flexDirection: "column", flex: 1 }}>
      <div
        style={{
          padding: 12,
          borderBottom: "1px solid var(--celerex-border)",
          display: "flex",
          gap: 8,
          alignItems: "center",
        }}
      >
        <input
          data-testid="agent-name-input"
          value={name}
          onChange={(e) => setName(e.target.value)}
          style={{
            padding: "6px 8px",
            borderRadius: 6,
            border: "1px solid var(--celerex-border)",
            fontSize: 13,
            minWidth: 200,
          }}
        />
        <button
          data-testid="save-agent"
          onClick={() => void handleSave()}
          disabled={saving}
          style={{
            padding: "6px 12px",
            borderRadius: 6,
            border: "none",
            background: "var(--celerex-accent)",
            color: "#fff",
            cursor: saving ? "not-allowed" : "pointer",
            fontSize: 13,
            fontWeight: 500,
            opacity: saving ? 0.6 : 1,
          }}
        >
          {saving ? "Saving…" : activeMemberId ? "Save" : "Create"}
        </button>
        <button
          data-testid="run-now"
          onClick={() => void onRunNow()}
          disabled={running}
          style={{
            ...toolbarButtonStyle(),
            opacity: running ? 0.6 : 1,
            cursor: running ? "not-allowed" : "pointer",
          }}
        >
          {running ? "Starting…" : "Run now"}
        </button>
        <div style={{ flex: 1 }} />
        {activeMemberId && (
          <div style={{ display: "flex", gap: 4 }}>
            <button
              data-testid="tab-config"
              onClick={() => setTab("config")}
              style={toolbarButtonStyle(tab === "config")}
            >
              Config
            </button>
            <button
              data-testid="tab-schedules"
              onClick={() => setTab("schedules")}
              style={toolbarButtonStyle(tab === "schedules")}
            >
              Schedules
            </button>
          </div>
        )}
      </div>
      <div style={{ flex: 1, display: "flex", overflow: "hidden" }}>
        <AgentBuilderCanvas graph={graph} mode="edit" onEdit={setGraph} />
        <div
          style={{
            width: 360,
            display: "flex",
            flexDirection: "column",
            borderLeft: "1px solid var(--celerex-border)",
            overflow: "hidden",
          }}
        >
          {tab === "config" || !activeMemberId ? (
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
          ) : (
            <SchedulesPanel token={token} projectMemberId={activeMemberId} />
          )}
        </div>
      </div>
    </div>
  );
}
