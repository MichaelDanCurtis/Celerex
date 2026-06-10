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
import { RunHistoryPanel } from "../components/RunHistoryPanel";

type RightTab = "config" | "schedules" | "runs";

export function BuilderPage({
  projectId,
  token,
}: {
  projectId: string;
  token: string;
}) {
  const [graph, setGraph] = useState<Graph>({ nodes: [], edges: [] });
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [name, setName] = useState("Untitled flow");
  const setMemberId = useSetAtom(activeMemberIdAtom);
  const setRun = useSetAtom(activeRunIdAtom);
  const [, setView] = useAtom(activeViewAtom);
  const addToast = useSetAtom(addToastAtom);
  const activeMemberId = useAtomValue(activeMemberIdAtom);
  const [providerConfig, setProviderConfig] = useState<Record<string, unknown>>({});
  const [tab, setTab] = useState<RightTab>("config");
  const [saving, setSaving] = useState(false);
  const [running, setRunning] = useState(false);

  // Hydrate canvas when an existing flow is selected.
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
            position: (n as { position?: { x: number; y: number } }).position,
          })),
          edges: m.graph_json.edges,
        });
        setProviderConfig(m.provider_config_json ?? {});
      })
      .catch((err) => {
        addToast({
          message: err instanceof Error ? err.message : "Failed to load flow",
          type: "error",
        });
      });
    return () => {
      cancelled = true;
    };
  }, [activeMemberId, token, addToast]);

  const selected: GraphNode | null = selectedId
    ? graph.nodes.find((n) => n.id === selectedId) ?? null
    : null;
  const upstreamIds = selected
    ? graph.nodes.filter((n) => n.id !== selected.id).map((n) => n.id)
    : [];

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
  }, [projectId, activeMemberId, token, name, graph, providerConfig, setMemberId, addToast]);

  const onRunNow = useCallback(async () => {
    setRunning(true);
    try {
      let id = activeMemberId;
      // Persist latest graph before running so the run reflects on-screen edits.
      if (!id) {
        const m = await createMember(token, {
          project_id: projectId,
          name,
          graph_json: graph,
          provider_config_json: providerConfig,
        });
        setMemberId(m.id);
        id = m.id;
      } else {
        await updateMember(token, id, { name, graph_json: graph, provider_config_json: providerConfig });
      }
      const { run_id } = await runMemberNow(token, id!);
      setRun(run_id);
      addToast({ message: "Run started", type: "success" });
      setView("monitor");
    } catch (err) {
      addToast({
        message: err instanceof Error ? err.message : "Run failed",
        type: "error",
      });
    } finally {
      setRunning(false);
    }
  }, [activeMemberId, token, projectId, name, graph, providerConfig, setMemberId, setRun, setView, addToast]);

  const tabBtn = (active = false) =>
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
    <div style={{ display: "flex", flexDirection: "column", flex: 1, minHeight: 0 }}>
      <div
        style={{
          padding: "10px 12px",
          borderBottom: "1px solid var(--celerex-border)",
          display: "flex",
          gap: 8,
          alignItems: "center",
          background: "var(--celerex-bg-primary)",
        }}
      >
        <button onClick={() => setView("flows")} style={tabBtn()} title="Back to flows">
          ← Flows
        </button>
        <input
          data-testid="agent-name-input"
          value={name}
          onChange={(e) => setName(e.target.value)}
          style={{
            padding: "6px 8px",
            borderRadius: 6,
            border: "1px solid var(--celerex-border)",
            background: "var(--celerex-bg-primary)",
            color: "var(--celerex-text-primary)",
            fontSize: 14,
            fontWeight: 600,
            minWidth: 220,
          }}
        />
        <button
          data-testid="save-agent"
          onClick={() => void handleSave()}
          disabled={saving}
          style={{
            padding: "6px 14px",
            borderRadius: 6,
            border: "none",
            background: "var(--celerex-accent)",
            color: "#fff",
            cursor: saving ? "not-allowed" : "pointer",
            fontSize: 13,
            fontWeight: 600,
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
            padding: "6px 14px",
            borderRadius: 6,
            border: "none",
            background: "#16a34a",
            color: "#fff",
            opacity: running ? 0.6 : 1,
            cursor: running ? "not-allowed" : "pointer",
            fontSize: 13,
            fontWeight: 600,
          }}
        >
          {running ? "Starting…" : "▶ Run"}
        </button>
        <div style={{ flex: 1 }} />
        {activeMemberId && (
          <div style={{ display: "flex", gap: 4 }}>
            <button data-testid="tab-config" onClick={() => setTab("config")} style={tabBtn(tab === "config")}>
              Config
            </button>
            <button data-testid="tab-schedules" onClick={() => setTab("schedules")} style={tabBtn(tab === "schedules")}>
              Schedules
            </button>
            <button data-testid="tab-runs" onClick={() => setTab("runs")} style={tabBtn(tab === "runs")}>
              Runs
            </button>
          </div>
        )}
      </div>
      <div style={{ flex: 1, display: "flex", overflow: "hidden", minHeight: 0 }}>
        <AgentBuilderCanvas
          graph={graph}
          mode="edit"
          onEdit={setGraph}
          onNodeClick={(id) => {
            setSelectedId(id);
            setTab("config");
          }}
        />
        <div
          style={{
            width: 340,
            flexShrink: 0,
            display: "flex",
            flexDirection: "column",
            borderLeft: "1px solid var(--celerex-border)",
            overflow: "hidden",
            background: "var(--celerex-bg-primary)",
          }}
        >
          {tab === "config" || !activeMemberId ? (
            <NodeConfigPanel
              node={selected}
              upstreamIds={upstreamIds}
              onChange={(n) => {
                setGraph((prev) => ({
                  ...prev,
                  nodes: prev.nodes.map((x) => (x.id === n.id ? n : x)),
                }));
              }}
            />
          ) : tab === "schedules" ? (
            <SchedulesPanel token={token} projectMemberId={activeMemberId} />
          ) : (
            <RunHistoryPanel token={token} projectMemberId={activeMemberId} />
          )}
        </div>
      </div>
    </div>
  );
}
