import { useCallback, useEffect, useMemo, useState } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import {
  engagementsAtom,
  activeProjectIdAtom,
  projectsAtom,
  tokenAtom,
  type Engagement,
  type Project,
} from "../state/atoms";
import { addToastAtom } from "../state/toastAtom";
import {
  createEngagement as apiCreateEngagement,
  deleteEngagement as apiDeleteEngagement,
  createProject as apiCreateProject,
  fetchEngagements,
  listProjects,
  type ApiEngagement,
  type ApiProject,
} from "../api/client";

function mapApiEngagement(api: ApiEngagement): Engagement {
  return {
    id: api.id,
    name: api.name,
    createdAt: new Date(api.created_at).getTime(),
    docs: api.docs.map((d) => ({
      id: d.id,
      title: d.title,
      engagementId: d.engagement_id,
      createdAt: new Date(d.created_at).getTime(),
    })),
  };
}

function mapApiProject(api: ApiProject): Project {
  return { id: api.id, name: api.name, engagementId: api.engagement_id };
}

const overlayStyle: React.CSSProperties = {
  position: "fixed",
  inset: 0,
  zIndex: 9000,
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  background: "rgba(0,0,0,0.5)",
  backdropFilter: "blur(3px)",
};

const dialogStyle: React.CSSProperties = {
  width: 720,
  maxWidth: "92vw",
  maxHeight: "85vh",
  borderRadius: 12,
  background: "var(--celerex-bg-primary)",
  border: "1px solid var(--celerex-border)",
  boxShadow: "0 20px 40px rgba(0,0,0,0.18)",
  display: "flex",
  flexDirection: "column",
  overflow: "hidden",
};

const headerStyle: React.CSSProperties = {
  padding: "12px 16px",
  borderBottom: "1px solid var(--celerex-border)",
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
};

const sectionTitleStyle: React.CSSProperties = {
  fontSize: 11,
  fontWeight: 600,
  textTransform: "uppercase",
  letterSpacing: "0.05em",
  color: "var(--celerex-text-secondary)",
  marginBottom: 6,
};

const listItemStyle = (active: boolean): React.CSSProperties => ({
  display: "flex",
  alignItems: "center",
  gap: 6,
  padding: "6px 8px",
  borderRadius: 6,
  fontSize: 13,
  cursor: "pointer",
  background: active ? "rgba(59, 130, 246, 0.10)" : "transparent",
  color: active ? "var(--celerex-accent)" : "var(--celerex-text-primary)",
  fontWeight: active ? 500 : 400,
});

const primaryButtonStyle: React.CSSProperties = {
  padding: "6px 12px",
  borderRadius: 6,
  border: "none",
  background: "var(--celerex-accent)",
  color: "#fff",
  cursor: "pointer",
  fontSize: 13,
  fontWeight: 500,
};

const secondaryButtonStyle: React.CSSProperties = {
  padding: "6px 12px",
  borderRadius: 6,
  border: "1px solid var(--celerex-border)",
  background: "transparent",
  color: "var(--celerex-text-secondary)",
  cursor: "pointer",
  fontSize: 13,
};

const inputStyle: React.CSSProperties = {
  flex: 1,
  padding: "6px 8px",
  borderRadius: 6,
  border: "1px solid var(--celerex-border)",
  fontSize: 13,
  background: "var(--celerex-bg-primary)",
  color: "var(--celerex-text-primary)",
  outline: "none",
};

export function EngagementManager({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  const [engagements, setEngagements] = useAtom(engagementsAtom);
  const [projects, setProjects] = useAtom(projectsAtom);
  const [activeProjectId, setActiveProjectId] = useAtom(activeProjectIdAtom);
  const token = useAtomValue(tokenAtom);
  const addToast = useSetAtom(addToastAtom);

  const [selectedEngagementId, setSelectedEngagementId] = useState<
    string | null
  >(null);
  const [newEngagementName, setNewEngagementName] = useState("");
  const [newProjectName, setNewProjectName] = useState("");
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);

  // Resolve the project's parent engagement so we can default-select an
  // engagement when the dialog opens.
  const activeProject = useMemo(
    () => projects.find((p) => p.id === activeProjectId) ?? null,
    [projects, activeProjectId],
  );

  useEffect(() => {
    if (!open) return;
    if (selectedEngagementId) return;
    if (activeProject) {
      setSelectedEngagementId(activeProject.engagementId);
    } else if (engagements.length > 0) {
      setSelectedEngagementId(engagements[0].id);
    }
  }, [open, selectedEngagementId, activeProject, engagements]);

  const refreshAll = useCallback(async () => {
    setLoading(true);
    try {
      const [apiEngs, apiProjects] = await Promise.all([
        fetchEngagements().catch((e) => {
          console.warn("[EngagementManager] fetchEngagements failed:", e);
          return [] as ApiEngagement[];
        }),
        token
          ? listProjects(token).catch((e) => {
              console.warn("[EngagementManager] listProjects failed:", e);
              return [] as ApiProject[];
            })
          : Promise.resolve([] as ApiProject[]),
      ]);
      if (apiEngs.length > 0) {
        setEngagements(apiEngs.map(mapApiEngagement));
      }
      setProjects(apiProjects.map(mapApiProject));
    } finally {
      setLoading(false);
    }
  }, [token, setEngagements, setProjects]);

  useEffect(() => {
    if (!open) return;
    void refreshAll();
  }, [open, refreshAll]);

  const visibleProjects = useMemo(
    () =>
      selectedEngagementId
        ? projects.filter((p) => p.engagementId === selectedEngagementId)
        : [],
    [projects, selectedEngagementId],
  );

  const handleCreateEngagement = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault();
      const name = newEngagementName.trim();
      if (!name) return;
      setBusy(true);
      try {
        const created = await apiCreateEngagement(name);
        const mapped = mapApiEngagement(created);
        setEngagements((prev) => [...prev, mapped]);
        setSelectedEngagementId(mapped.id);
        setNewEngagementName("");
        addToast({ message: "Engagement created", type: "success" });
      } catch (err) {
        addToast({
          message:
            err instanceof Error ? err.message : "Failed to create engagement",
          type: "error",
        });
      } finally {
        setBusy(false);
      }
    },
    [newEngagementName, setEngagements, addToast],
  );

  const handleDeleteEngagement = useCallback(
    async (engId: string) => {
      if (
        typeof window !== "undefined" &&
        !window.confirm("Delete this engagement and all its docs?")
      ) {
        return;
      }
      setBusy(true);
      try {
        await apiDeleteEngagement(engId);
        setEngagements((prev) => prev.filter((e) => e.id !== engId));
        setProjects((prev) => prev.filter((p) => p.engagementId !== engId));
        if (selectedEngagementId === engId) {
          setSelectedEngagementId(null);
        }
        if (activeProject?.engagementId === engId) {
          setActiveProjectId(null);
        }
        addToast({ message: "Engagement deleted", type: "info" });
      } catch (err) {
        addToast({
          message:
            err instanceof Error ? err.message : "Failed to delete engagement",
          type: "error",
        });
      } finally {
        setBusy(false);
      }
    },
    [
      setEngagements,
      setProjects,
      selectedEngagementId,
      activeProject,
      setActiveProjectId,
      addToast,
    ],
  );

  const handleCreateProject = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault();
      const name = newProjectName.trim();
      if (!name) return;
      if (!token) {
        addToast({
          message: "Sign in required to create a project",
          type: "error",
        });
        return;
      }
      if (!selectedEngagementId) {
        addToast({ message: "Select an engagement first", type: "error" });
        return;
      }
      setBusy(true);
      try {
        // NB: backend's createProject derives the engagement from the user's
        // JWT claims today, so we don't pass engagement_id explicitly here.
        const created = await apiCreateProject(token, name);
        const mapped = mapApiProject(created);
        setProjects((prev) => [...prev, mapped]);
        setActiveProjectId(mapped.id);
        setNewProjectName("");
        addToast({ message: "Project created", type: "success" });
      } catch (err) {
        addToast({
          message:
            err instanceof Error ? err.message : "Failed to create project",
          type: "error",
        });
      } finally {
        setBusy(false);
      }
    },
    [
      newProjectName,
      token,
      selectedEngagementId,
      setProjects,
      setActiveProjectId,
      addToast,
    ],
  );

  if (!open) return null;

  return (
    <div
      style={overlayStyle}
      onClick={onClose}
      data-testid="engagement-manager-overlay"
    >
      <div
        style={dialogStyle}
        onClick={(e) => e.stopPropagation()}
        data-testid="engagement-manager"
      >
        <div style={headerStyle}>
          <h2 style={{ margin: 0, fontSize: 16, fontWeight: 600 }}>
            Engagements & Projects
          </h2>
          <button
            onClick={onClose}
            data-testid="engagement-manager-close"
            style={{
              ...secondaryButtonStyle,
              padding: "4px 10px",
            }}
          >
            Close
          </button>
        </div>

        {!token && (
          <div
            data-testid="engagement-manager-signin-gate"
            style={{
              padding: "10px 16px",
              background: "rgba(217, 119, 6, 0.10)",
              color: "#b45309",
              fontSize: 12,
              borderBottom: "1px solid var(--celerex-border)",
            }}
          >
            Not signed in. You can browse engagements but creating projects
            requires authentication.
          </div>
        )}

        <div
          style={{
            display: "flex",
            flex: 1,
            minHeight: 0,
          }}
        >
          {/* Engagements column */}
          <div
            style={{
              width: 280,
              borderRight: "1px solid var(--celerex-border)",
              padding: 12,
              display: "flex",
              flexDirection: "column",
              gap: 8,
              overflow: "auto",
            }}
          >
            <div style={sectionTitleStyle}>Engagements</div>

            {loading && (
              <div style={{ fontSize: 12, opacity: 0.6 }}>Loading…</div>
            )}

            {!loading && engagements.length === 0 && (
              <div
                data-testid="engagements-empty"
                style={{
                  fontSize: 13,
                  opacity: 0.7,
                  padding: "12px 0",
                }}
              >
                Create your first engagement
              </div>
            )}

            <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
              {engagements.map((eng) => {
                const active = eng.id === selectedEngagementId;
                return (
                  <div
                    key={eng.id}
                    data-testid={`engagement-row-${eng.id}`}
                    style={listItemStyle(active)}
                    onClick={() => setSelectedEngagementId(eng.id)}
                  >
                    <span
                      style={{
                        flex: 1,
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {eng.name}
                    </span>
                    <button
                      data-testid={`delete-engagement-${eng.id}`}
                      onClick={(e) => {
                        e.stopPropagation();
                        void handleDeleteEngagement(eng.id);
                      }}
                      disabled={busy}
                      title="Delete engagement"
                      style={{
                        background: "transparent",
                        border: "none",
                        cursor: busy ? "not-allowed" : "pointer",
                        fontSize: 14,
                        lineHeight: 1,
                        opacity: 0.5,
                        color: "var(--celerex-text-secondary)",
                      }}
                    >
                      ×
                    </button>
                  </div>
                );
              })}
            </div>

            <form
              data-testid="create-engagement-form"
              onSubmit={(e) => void handleCreateEngagement(e)}
              style={{
                display: "flex",
                gap: 6,
                marginTop: 6,
              }}
            >
              <input
                data-testid="new-engagement-name"
                value={newEngagementName}
                onChange={(e) => setNewEngagementName(e.target.value)}
                placeholder="Engagement name…"
                style={inputStyle}
              />
              <button
                type="submit"
                data-testid="create-engagement-submit"
                disabled={busy || !newEngagementName.trim()}
                style={{
                  ...primaryButtonStyle,
                  opacity:
                    busy || !newEngagementName.trim() ? 0.6 : 1,
                  cursor:
                    busy || !newEngagementName.trim()
                      ? "not-allowed"
                      : "pointer",
                }}
              >
                Create
              </button>
            </form>
          </div>

          {/* Projects column */}
          <div
            style={{
              flex: 1,
              padding: 12,
              display: "flex",
              flexDirection: "column",
              gap: 8,
              overflow: "auto",
              minWidth: 0,
            }}
          >
            <div style={sectionTitleStyle}>
              Projects
              {selectedEngagementId && (
                <span
                  style={{
                    marginLeft: 6,
                    opacity: 0.7,
                    fontWeight: 400,
                    textTransform: "none",
                    letterSpacing: 0,
                  }}
                >
                  in{" "}
                  {engagements.find((e) => e.id === selectedEngagementId)
                    ?.name ?? "—"}
                </span>
              )}
            </div>

            {!selectedEngagementId && (
              <div
                style={{
                  fontSize: 13,
                  opacity: 0.7,
                  padding: "12px 0",
                }}
              >
                Select an engagement to see its projects.
              </div>
            )}

            {selectedEngagementId && visibleProjects.length === 0 && (
              <div
                data-testid="projects-empty"
                style={{
                  fontSize: 13,
                  opacity: 0.7,
                  padding: "12px 0",
                }}
              >
                Create your first project
              </div>
            )}

            <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
              {visibleProjects.map((p) => {
                const active = p.id === activeProjectId;
                return (
                  <div
                    key={p.id}
                    data-testid={`project-row-${p.id}`}
                    style={listItemStyle(active)}
                    onClick={() => setActiveProjectId(p.id)}
                  >
                    <span
                      style={{
                        flex: 1,
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {p.name}
                    </span>
                    {active && (
                      <span
                        style={{
                          fontSize: 11,
                          color: "var(--celerex-accent)",
                          fontWeight: 500,
                        }}
                      >
                        active
                      </span>
                    )}
                  </div>
                );
              })}
            </div>

            {selectedEngagementId && (
              <form
                data-testid="create-project-form"
                onSubmit={(e) => void handleCreateProject(e)}
                style={{
                  display: "flex",
                  gap: 6,
                  marginTop: 6,
                }}
              >
                <input
                  data-testid="new-project-name"
                  value={newProjectName}
                  onChange={(e) => setNewProjectName(e.target.value)}
                  placeholder="Project name…"
                  style={inputStyle}
                  disabled={!token}
                />
                <button
                  type="submit"
                  data-testid="create-project-submit"
                  disabled={busy || !newProjectName.trim() || !token}
                  style={{
                    ...primaryButtonStyle,
                    opacity:
                      busy || !newProjectName.trim() || !token ? 0.6 : 1,
                    cursor:
                      busy || !newProjectName.trim() || !token
                        ? "not-allowed"
                        : "pointer",
                  }}
                  title={!token ? "Sign in required" : undefined}
                >
                  Create
                </button>
              </form>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
