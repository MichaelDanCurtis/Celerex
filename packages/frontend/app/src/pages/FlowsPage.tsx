import { useCallback, useEffect, useState } from "react";
import { useSetAtom } from "jotai";
import {
  activeMemberIdAtom,
  activeProjectIdAtom,
  activeRunIdAtom,
  activeViewAtom,
} from "../state/atoms";
import { addToastAtom } from "../state/toastAtom";
import {
  createMember,
  createProject,
  deleteMember,
  listMembers,
  listProjects,
  runMemberNow,
  type ApiProjectMember,
} from "../api/client";
import { FLOW_TEMPLATES, type FlowTemplate } from "../canvas/flowTemplates";
import { CATEGORY_META } from "../canvas/palette";

export function FlowsPage({ token }: { token: string }) {
  const setMemberId = useSetAtom(activeMemberIdAtom);
  const setView = useSetAtom(activeViewAtom);
  const setRun = useSetAtom(activeRunIdAtom);
  const setProjectId = useSetAtom(activeProjectIdAtom);
  const addToast = useSetAtom(addToastAtom);

  const [projectId, setLocalProjectId] = useState<string | null>(null);
  const [flows, setFlows] = useState<ApiProjectMember[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);

  // Bootstrap: make sure there's a project to hang flows off, then load them.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        let projects = await listProjects(token);
        if (projects.length === 0) {
          await createProject(token, "Workspace");
          projects = await listProjects(token);
        }
        const pid = projects[0]?.id ?? null;
        if (cancelled) return;
        setLocalProjectId(pid);
        if (pid) {
          setProjectId(pid);
          const members = await listMembers(token, pid);
          if (!cancelled) setFlows(members);
        }
      } catch (err) {
        if (!cancelled)
          addToast({
            message: err instanceof Error ? err.message : "Failed to load flows",
            type: "error",
          });
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [token, setProjectId, addToast]);

  const refresh = useCallback(async () => {
    if (!projectId) return;
    try {
      setFlows(await listMembers(token, projectId));
    } catch {
      /* non-fatal */
    }
  }, [projectId, token]);

  const createFromTemplate = useCallback(
    async (t: FlowTemplate) => {
      if (!projectId) {
        addToast({ message: "Still setting up your workspace…", type: "info" });
        return;
      }
      setBusy(true);
      try {
        const m = await createMember(token, {
          project_id: projectId,
          name: t.id === "blank" ? "Untitled flow" : t.name,
          graph_json: t.build(),
        });
        setMemberId(m.id);
        setView("builder");
        addToast({ message: `Created “${m.name}”`, type: "success" });
      } catch (err) {
        addToast({
          message: err instanceof Error ? err.message : "Create failed",
          type: "error",
        });
      } finally {
        setBusy(false);
      }
    },
    [projectId, token, setMemberId, setView, addToast],
  );

  const openFlow = useCallback(
    (m: ApiProjectMember) => {
      setMemberId(m.id);
      setView("builder");
    },
    [setMemberId, setView],
  );

  const removeFlow = useCallback(
    async (m: ApiProjectMember) => {
      if (!window.confirm(`Delete “${m.name}”? Its run history and schedules go with it.`)) return;
      try {
        await deleteMember(token, m.id);
        setFlows((prev) => prev.filter((f) => f.id !== m.id));
        addToast({ message: `Deleted “${m.name}”`, type: "success" });
      } catch (err) {
        addToast({
          message: err instanceof Error ? err.message : "Delete failed",
          type: "error",
        });
      }
    },
    [token, addToast],
  );

  const runFlow = useCallback(
    async (m: ApiProjectMember) => {
      try {
        setMemberId(m.id);
        const { run_id } = await runMemberNow(token, m.id);
        setRun(run_id);
        setView("monitor");
        addToast({ message: `Running “${m.name}”`, type: "success" });
      } catch (err) {
        addToast({
          message: err instanceof Error ? err.message : "Run failed",
          type: "error",
        });
      }
    },
    [token, setMemberId, setRun, setView, addToast],
  );

  return (
    <div style={{ flex: 1, overflowY: "auto", padding: "28px 32px", maxWidth: 1100, margin: "0 auto", width: "100%" }}>
      <header style={{ marginBottom: 8, display: "flex", alignItems: "flex-start", gap: 16 }}>
        <div style={{ flex: 1 }}>
          <h1 style={{ fontSize: 26, fontWeight: 700, marginBottom: 4 }}>Flows</h1>
          <p style={{ color: "var(--celerex-text-secondary)", fontSize: 14 }}>
            Wire triggers, AI agents, documents, and connectors into automations — n8n-style, on a visual canvas.
          </p>
        </div>
        <button onClick={() => setView("network")} style={ghostBtn} title="See all flows as a living 3D graph">
          🌐 View network
        </button>
      </header>

      {/* Templates */}
      <section style={{ marginTop: 24 }}>
        <SectionTitle>Start from a template</SectionTitle>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fill, minmax(240px, 1fr))",
            gap: 14,
          }}
        >
          {FLOW_TEMPLATES.map((t) => (
            <button
              key={t.id}
              disabled={busy}
              onClick={() => void createFromTemplate(t)}
              style={cardStyle(busy)}
            >
              <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 8 }}>
                <span style={iconBadge}>{t.icon}</span>
                <span style={{ fontWeight: 600, fontSize: 14.5 }}>{t.name}</span>
              </div>
              <div style={{ fontSize: 12.5, color: "var(--celerex-text-secondary)", lineHeight: 1.45 }}>
                {t.description}
              </div>
            </button>
          ))}
        </div>
      </section>

      {/* Existing flows */}
      <section style={{ marginTop: 32 }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
          <SectionTitle>Your flows</SectionTitle>
          <button onClick={() => void refresh()} style={ghostBtn}>
            Refresh
          </button>
        </div>
        {loading ? (
          <Empty>Loading…</Empty>
        ) : flows.length === 0 ? (
          <Empty>No flows yet — pick a template above to create your first one.</Empty>
        ) : (
          <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(260px, 1fr))", gap: 14 }}>
            {flows.map((m) => {
              const nodeCount = m.graph_json?.nodes?.length ?? 0;
              const edgeCount = m.graph_json?.edges?.length ?? 0;
              const cats = new Set((m.graph_json?.nodes ?? []).map((n) => n.type));
              return (
                <div key={m.id} style={{ ...cardStyle(false), cursor: "default" }}>
                  <div style={{ fontWeight: 600, fontSize: 14.5, marginBottom: 6 }}>{m.name}</div>
                  <div style={{ display: "flex", gap: 5, marginBottom: 10 }}>
                    {[...cats].map((c) => (
                      <span
                        key={c}
                        title={c}
                        style={{
                          width: 9,
                          height: 9,
                          borderRadius: "50%",
                          background: CATEGORY_META[c as keyof typeof CATEGORY_META]?.color ?? "#888",
                        }}
                      />
                    ))}
                    <span style={{ fontSize: 11.5, color: "var(--celerex-text-secondary)", marginLeft: 4 }}>
                      {nodeCount} node{nodeCount === 1 ? "" : "s"} · {edgeCount} link{edgeCount === 1 ? "" : "s"}
                    </span>
                  </div>
                  <div style={{ display: "flex", gap: 8 }}>
                    <button onClick={() => openFlow(m)} style={primaryBtn}>
                      Open
                    </button>
                    <button onClick={() => void runFlow(m)} style={ghostBtn}>
                      ▶ Run
                    </button>
                    <div style={{ flex: 1 }} />
                    <button
                      onClick={() => void removeFlow(m)}
                      title="Delete flow"
                      style={{ ...ghostBtn, color: "#ef4444", borderColor: "#ef444455" }}
                    >
                      🗑
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </section>
    </div>
  );
}

function SectionTitle({ children }: { children: React.ReactNode }) {
  return (
    <h2 style={{ fontSize: 12, fontWeight: 700, textTransform: "uppercase", letterSpacing: 0.8, color: "var(--celerex-text-secondary)", marginBottom: 12 }}>
      {children}
    </h2>
  );
}

function Empty({ children }: { children: React.ReactNode }) {
  return (
    <div
      style={{
        padding: "28px 20px",
        textAlign: "center",
        color: "var(--celerex-text-secondary)",
        fontSize: 13,
        border: "1px dashed var(--celerex-border)",
        borderRadius: 12,
      }}
    >
      {children}
    </div>
  );
}

const iconBadge: React.CSSProperties = {
  width: 34,
  height: 34,
  borderRadius: 9,
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  fontSize: 18,
  background: "var(--celerex-bg-secondary)",
  border: "1px solid var(--celerex-border)",
};

function cardStyle(disabled: boolean): React.CSSProperties {
  return {
    textAlign: "left",
    padding: 16,
    borderRadius: 12,
    border: "1px solid var(--celerex-border)",
    background: "var(--celerex-bg-primary)",
    cursor: disabled ? "wait" : "pointer",
    opacity: disabled ? 0.7 : 1,
    boxShadow: "0 1px 3px rgba(0,0,0,0.04)",
    transition: "border-color 120ms ease, box-shadow 120ms ease",
  };
}

const primaryBtn: React.CSSProperties = {
  padding: "6px 14px",
  borderRadius: 7,
  border: "none",
  background: "var(--celerex-accent)",
  color: "#fff",
  fontSize: 12.5,
  fontWeight: 600,
  cursor: "pointer",
};

const ghostBtn: React.CSSProperties = {
  padding: "6px 12px",
  borderRadius: 7,
  border: "1px solid var(--celerex-border)",
  background: "transparent",
  color: "var(--celerex-text-secondary)",
  fontSize: 12.5,
  fontWeight: 500,
  cursor: "pointer",
};
