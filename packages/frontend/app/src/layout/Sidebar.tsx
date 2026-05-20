import { useState, useRef, useEffect, useMemo } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import {
  engagementsAtom,
  activeDocIdAtom,
  openDocAtom,
  activeViewAtom,
  favoritesAtom,
  toggleFavoriteAtom,
  recentDocIdsAtom,
  trackRecentDocAtom,
  projectsAtom,
  activeProjectIdAtom,
  engagementsForActiveProjectAtom,
  genId,
  tokenAtom,
  authEngagementAtom,
  type Engagement,
} from "../state/atoms";
import { addToastAtom } from "../state/toastAtom";
import { SearchPanel } from "./SearchPanel";
import { EngagementManager } from "./EngagementManager";
import { useProjectsSync } from "../hooks/useProjectsSync";

export function Sidebar({
  collapsed,
  onToggleCollapse,
}: {
  collapsed: boolean;
  onToggleCollapse: () => void;
}) {
  const [engagements, setEngagements] = useAtom(engagementsAtom);
  const projects = useAtomValue(projectsAtom);
  const [activeProjectId, setActiveProjectId] = useAtom(activeProjectIdAtom);
  const visibleEngagements = useAtomValue(engagementsForActiveProjectAtom);
  const [activeDocId, setActiveDocId] = useAtom(activeDocIdAtom);
  const openDoc = useSetAtom(openDocAtom);
  const [activeView, setActiveView] = useAtom(activeViewAtom);
  const addToast = useSetAtom(addToastAtom);
  const favorites = useAtomValue(favoritesAtom);
  const toggleFavorite = useSetAtom(toggleFavoriteAtom);
  const recentDocIds = useAtomValue(recentDocIdsAtom);
  const trackRecentDoc = useSetAtom(trackRecentDocAtom);
  const [creatingEngagement, setCreatingEngagement] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [showSearchPanel, setShowSearchPanel] = useState(false);
  const [engagementManagerOpen, setEngagementManagerOpen] = useState(false);
  const setToken = useSetAtom(tokenAtom);
  const [authEngagement, setAuthEngagement] = useAtom(authEngagementAtom);
  // Sync the projects atom with the backend on mount (and exposes refresh()).
  useProjectsSync();

  function handleSignOut() {
    // Clearing the token sends App.tsx back to the LoginPage. atomWithStorage
    // takes care of wiping localStorage too.
    setToken("");
    setAuthEngagement(null);
  }

  const favoriteDocs = useMemo(() => {
    const allDocs = engagements.flatMap((e) => e.docs);
    return favorites
      .map((id) => allDocs.find((d) => d.id === id))
      .filter(Boolean) as typeof allDocs;
  }, [favorites, engagements]);

  const recentDocs = useMemo(() => {
    const allDocs = engagements.flatMap((e) => e.docs);
    const favSet = new Set(favorites);
    return recentDocIds
      .filter((id) => !favSet.has(id))
      .slice(0, 5)
      .map((id) => allDocs.find((d) => d.id === id))
      .filter(Boolean) as typeof allDocs;
  }, [recentDocIds, favorites, engagements]);

  function handleCreateEngagement(name: string) {
    const eng: Engagement = {
      id: genId("eng"),
      name,
      createdAt: Date.now(),
      docs: [],
    };
    setEngagements((prev) => [...prev, eng]);
    setCreatingEngagement(false);
    addToast({ message: "Workspace created", type: "success" });
  }

  function handleCreateDoc(engagementId: string, title: string) {
    const docId = genId("doc");
    setEngagements((prev) =>
      prev.map((eng) =>
        eng.id === engagementId
          ? {
              ...eng,
              docs: [
                ...eng.docs,
                { id: docId, title, engagementId, createdAt: Date.now() },
              ],
            }
          : eng
      )
    );
    setActiveDocId(docId);
    trackRecentDoc(docId);
  }

  function handleDuplicateDoc(engagementId: string, docId: string) {
    const eng = engagements.find((e) => e.id === engagementId);
    if (!eng) return;
    const sourceDoc = eng.docs.find((d) => d.id === docId);
    if (!sourceDoc) return;
    const newDocId = genId("doc");
    const sourceIndex = eng.docs.indexOf(sourceDoc);
    setEngagements((prev) =>
      prev.map((e) => {
        if (e.id !== engagementId) return e;
        const docs = [...e.docs];
        docs.splice(sourceIndex + 1, 0, {
          id: newDocId,
          title: `${sourceDoc.title || "Untitled"} (copy)`,
          engagementId,
          createdAt: Date.now(),
        });
        return { ...e, docs };
      })
    );
    openDoc(newDocId);
    trackRecentDoc(newDocId);
  }

  function handleDeleteDoc(engagementId: string, docId: string) {
    setEngagements((prev) =>
      prev.map((eng) =>
        eng.id === engagementId
          ? { ...eng, docs: eng.docs.filter((d) => d.id !== docId) }
          : eng
      )
    );
    if (activeDocId === docId) {
      // Switch to first available doc
      const allDocs = engagements.flatMap((e) => e.docs).filter((d) => d.id !== docId);
      setActiveDocId(allDocs[0]?.id ?? "");
    }
    addToast({ message: "Document deleted", type: "info" });
  }

  function handleDeleteEngagement(engId: string) {
    const eng = engagements.find((e) => e.id === engId);
    if (!eng || eng.docs.length > 0) return; // Only delete empty engagements
    setEngagements((prev) => prev.filter((e) => e.id !== engId));
  }

  function handleRenameEngagement(engId: string, newName: string) {
    setEngagements((prev) =>
      prev.map((eng) =>
        eng.id === engId ? { ...eng, name: newName } : eng
      )
    );
  }

  function handleReorderDocs(engagementId: string, fromIndex: number, toIndex: number) {
    if (fromIndex === toIndex) return;
    setEngagements((prev) =>
      prev.map((eng) => {
        if (eng.id !== engagementId) return eng;
        const docs = [...eng.docs];
        const [moved] = docs.splice(fromIndex, 1);
        docs.splice(toIndex, 0, moved);
        return { ...eng, docs };
      })
    );
  }

  return (
    <aside
      style={{
        width: collapsed ? 48 : "var(--celerex-sidebar-width)",
        minWidth: collapsed ? 48 : undefined,
        height: "100%",
        borderRight: "1px solid var(--celerex-border)",
        background: "var(--celerex-bg-sidebar)",
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
      }}
    >
      <div
        style={{
          height: 48,
          display: "flex",
          alignItems: "center",
          justifyContent: collapsed ? "center" : "flex-start",
          padding: collapsed ? 0 : "0 16px",
          borderBottom: "1px solid var(--celerex-border)",
          fontWeight: 700,
          fontSize: 16,
          letterSpacing: "-0.02em",
        }}
      >
        {collapsed ? "C" : (
          <>
            <span style={{ flex: 1 }}>Celerex</span>
            <button
              onClick={(e) => {
                e.stopPropagation();
                setShowSearchPanel((v) => !v);
              }}
              title="Search All Docs"
              style={{
                background: showSearchPanel ? "rgba(59, 130, 246, 0.12)" : "transparent",
                border: "none",
                cursor: "pointer",
                fontSize: 15,
                lineHeight: 1,
                padding: "2px 4px",
                borderRadius: 4,
                color: showSearchPanel ? "var(--celerex-accent)" : "var(--celerex-text-secondary)",
                display: "inline-flex",
                alignItems: "center",
                justifyContent: "center",
                transition: "color 0.15s ease, background 0.15s ease",
              }}
            >
              {"\uD83D\uDD0D"}
            </button>
          </>
        )}
      </div>

      {!collapsed && (
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            padding: "6px 16px",
            borderBottom: "1px solid var(--celerex-border)",
            fontSize: 11,
            color: "var(--celerex-text-secondary)",
          }}
        >
          <span
            style={{
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
            title={authEngagement?.name ?? "Signed in"}
          >
            {authEngagement?.name ?? "Signed in"}
          </span>
          <button
            onClick={handleSignOut}
            title="Sign out"
            style={{
              background: "transparent",
              border: "1px solid var(--celerex-border)",
              borderRadius: 4,
              padding: "2px 8px",
              fontSize: 11,
              cursor: "pointer",
              color: "var(--celerex-text-secondary)",
              marginLeft: 8,
              flexShrink: 0,
            }}
          >
            Sign out
          </button>
        </div>
      )}

      {showSearchPanel && (
        <SearchPanel onClose={() => setShowSearchPanel(false)} />
      )}

      {!collapsed && (
        <div style={{ padding: "8px 8px 0" }}>
          <input
            type="text"
            placeholder="Search docs..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") setSearchQuery("");
            }}
            style={{
              display: "block",
              width: "100%",
              height: 30,
              padding: "0 8px",
              fontSize: 12,
              border: "1px solid var(--celerex-border)",
              borderRadius: 6,
              outline: "none",
              background: "var(--celerex-bg-primary)",
              color: "var(--celerex-text-primary)",
              boxSizing: "border-box",
            }}
          />
        </div>
      )}
      {!collapsed && (
        <div style={{ flex: 1, padding: "12px 8px", overflow: "auto" }}>
          {favoriteDocs.length > 0 && (
            <SidebarSection title="Favorites">
              {favoriteDocs.map((doc) => (
                <SidebarItem
                  key={doc.id}
                  label={doc.title || "Untitled"}
                  active={doc.id === activeDocId}
                  onClick={() => {
                    openDoc(doc.id);
                    trackRecentDoc(doc.id);
                    setActiveView("editor");
                    setSearchQuery("");
                  }}
                  onUnpin={() => toggleFavorite(doc.id)}
                />
              ))}
            </SidebarSection>
          )}
          {recentDocs.length > 0 && (
            <SidebarSection title="Recent">
              {recentDocs.map((doc) => (
                <SidebarItem
                  key={doc.id}
                  label={doc.title || "Untitled"}
                  active={doc.id === activeDocId}
                  onClick={() => {
                    openDoc(doc.id);
                    trackRecentDoc(doc.id);
                    setActiveView("editor");
                    setSearchQuery("");
                  }}
                />
              ))}
            </SidebarSection>
          )}
          <SidebarSection title="Projects">
            <SidebarItem
              label="All workspaces"
              active={activeProjectId === null}
              onClick={() => setActiveProjectId(null)}
            />
            {projects.map((p) => (
              <div
                key={p.id}
                style={{ display: "flex", alignItems: "center", gap: 4 }}
              >
                <div style={{ flex: 1 }}>
                  <SidebarItem
                    label={p.name}
                    active={activeProjectId === p.id}
                    onClick={() => setActiveProjectId(p.id)}
                  />
                </div>
                <button
                  data-testid={`add-agent-${p.id}`}
                  title="Add agent to project"
                  onClick={(e) => {
                    e.stopPropagation();
                    setActiveProjectId(p.id);
                    setActiveView("builder");
                  }}
                  style={{
                    fontSize: 11,
                    opacity: 0.7,
                    background: "transparent",
                    border: "none",
                    cursor: "pointer",
                    padding: "0 6px",
                  }}
                >
                  + Add agent
                </button>
              </div>
            ))}
            {projects.length === 0 && (
              <SidebarItem
                label="+ Create your first project"
                muted
                onClick={() => setEngagementManagerOpen(true)}
              />
            )}
            <SidebarItem
              label="Manage engagements & projects…"
              muted
              onClick={() => setEngagementManagerOpen(true)}
            />
          </SidebarSection>

          <SidebarSection title="Workspaces">
            {visibleEngagements
              .filter((eng) => {
                if (!searchQuery.trim()) return true;
                const q = searchQuery.trim().toLowerCase();
                return eng.docs.some((d) =>
                  (d.title || "Untitled").toLowerCase().includes(q)
                );
              })
              .map((eng) => {
                const filteredDocs = searchQuery.trim()
                  ? eng.docs.filter((d) =>
                      (d.title || "Untitled")
                        .toLowerCase()
                        .includes(searchQuery.trim().toLowerCase())
                    )
                  : eng.docs;
                return (
                  <EngagementGroup
                    key={eng.id}
                    engagement={{ ...eng, docs: filteredDocs }}
                    activeDocId={activeDocId}
                    favorites={favorites}
                    onToggleFavorite={toggleFavorite}
                    onSelectDoc={(id) => {
                      openDoc(id);
                      trackRecentDoc(id);
                      setActiveView("editor");
                      setSearchQuery("");
                    }}
                    onCreateDoc={(title) => handleCreateDoc(eng.id, title)}
                    onDeleteDoc={(docId) => handleDeleteDoc(eng.id, docId)}
                    onDuplicateDoc={(docId) => handleDuplicateDoc(eng.id, docId)}
                    onDeleteEngagement={() => handleDeleteEngagement(eng.id)}
                    onRename={(newName) =>
                      handleRenameEngagement(eng.id, newName)
                    }
                    onReorderDocs={(fromIndex, toIndex) =>
                      handleReorderDocs(eng.id, fromIndex, toIndex)
                    }
                  />
                );
              })}
            {creatingEngagement ? (
              <InlineInput
                placeholder="Workspace name..."
                onSubmit={handleCreateEngagement}
                onCancel={() => setCreatingEngagement(false)}
              />
            ) : (
              <SidebarItem
                label="+ New Workspace"
                muted
                onClick={() => setCreatingEngagement(true)}
              />
            )}
          </SidebarSection>

          <SidebarSection title="Views">
            <SidebarItem
              label="Dashboard"
              active={activeView === "dashboard"}
              onClick={() => setActiveView("dashboard")}
            />
            <SidebarItem
              label="Timeline"
              active={activeView === "timeline"}
              onClick={() => setActiveView("timeline")}
            />
            <SidebarItem
              label="Docs"
              active={activeView === "docs"}
              onClick={() => setActiveView("docs")}
            />
          </SidebarSection>

          <SidebarSection title="Agents">
            <SidebarItem
              label="Agent Dashboard"
              active={activeView === "agent-dashboard"}
              onClick={() => setActiveView("agent-dashboard")}
            />
            <SidebarItem
              label="Pipeline Editor"
              active={activeView === "pipeline-editor"}
              onClick={() => setActiveView("pipeline-editor")}
            />
            <SidebarItem
              label="Task Queue"
              active={activeView === "task-queue"}
              onClick={() => setActiveView("task-queue")}
            />
            <SidebarItem
              label="Agent Triggers"
              active={activeView === "agent-triggers"}
              onClick={() => setActiveView("agent-triggers")}
            />
            <SidebarItem
              label="Tools Manager"
              active={activeView === "tools-manager"}
              onClick={() => setActiveView("tools-manager")}
            />
          </SidebarSection>

          <SidebarSection title="Plugins">
            <SidebarItem
              label="Plugin Manager"
              active={activeView === "plugin-manager"}
              onClick={() => setActiveView("plugin-manager")}
            />
            <SidebarItem
              label="Plugins"
              active={activeView === "plugins"}
              onClick={() => setActiveView("plugins")}
            />
          </SidebarSection>

          <SidebarSection title="Workspace">
            <SidebarItem
              label="Settings"
              active={activeView === "settings"}
              onClick={() => setActiveView("settings")}
            />
          </SidebarSection>
        </div>
      )}
      {collapsed && <div style={{ flex: 1 }} />}

      <div
        onClick={onToggleCollapse}
        style={{
          padding: collapsed ? "8px 0" : "8px 16px",
          borderTop: "1px solid var(--celerex-border)",
          fontSize: 14,
          color: "var(--celerex-text-secondary)",
          cursor: "pointer",
          textAlign: collapsed ? "center" : "right",
          userSelect: "none",
        }}
        title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
      >
        {collapsed ? "\u00BB" : "\u00AB"}
      </div>
      <div
        style={{
          padding: collapsed ? "4px 0" : "4px 16px",
          borderTop: "1px solid var(--celerex-border)",
          fontSize: 12,
          color: "var(--celerex-text-secondary)",
          textAlign: collapsed ? "center" : "left",
          display: "flex",
          alignItems: "center",
          justifyContent: collapsed ? "center" : "space-between",
        }}
      >
        <span>{collapsed ? "v0.1" : "Celerex v0.1.0"}</span>
        <button
          onClick={(e) => {
            e.stopPropagation();
            setActiveView("settings");
          }}
          title="Settings"
          style={{
            background: activeView === "settings" ? "rgba(59, 130, 246, 0.12)" : "transparent",
            border: "none",
            cursor: "pointer",
            fontSize: 16,
            lineHeight: 1,
            padding: collapsed ? "2px 0" : "2px 4px",
            borderRadius: 4,
            color: activeView === "settings" ? "var(--celerex-accent)" : "var(--celerex-text-secondary)",
            display: "inline-flex",
            alignItems: "center",
            justifyContent: "center",
            transition: "color 0.15s ease, background 0.15s ease",
          }}
        >
          {"\u2699"}
        </button>
      </div>
      <EngagementManager
        open={engagementManagerOpen}
        onClose={() => setEngagementManagerOpen(false)}
      />
    </aside>
  );
}

function EngagementGroup({
  engagement,
  activeDocId,
  favorites,
  onToggleFavorite,
  onSelectDoc,
  onCreateDoc,
  onDeleteDoc,
  onDuplicateDoc,
  onDeleteEngagement,
  onRename,
  onReorderDocs,
}: {
  engagement: Engagement;
  activeDocId: string;
  favorites: string[];
  onToggleFavorite: (docId: string) => void;
  onSelectDoc: (id: string) => void;
  onCreateDoc: (title: string) => void;
  onDeleteDoc: (docId: string) => void;
  onDuplicateDoc: (docId: string) => void;
  onDeleteEngagement: () => void;
  onRename: (newName: string) => void;
  onReorderDocs: (fromIndex: number, toIndex: number) => void;
}) {
  const [collapsed, setCollapsed] = useState(false);
  const [creatingDoc, setCreatingDoc] = useState(false);
  const [editing, setEditing] = useState(false);
  const [dragOverDocId, setDragOverDocId] = useState<string | null>(null);
  const [dragPosition, setDragPosition] = useState<"above" | "below">("above");
  const [draggingDocId, setDraggingDocId] = useState<string | null>(null);

  function handleDragStart(e: React.DragEvent, docId: string, index: number) {
    setDraggingDocId(docId);
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", JSON.stringify({
      docId,
      engagementId: engagement.id,
      fromIndex: index,
    }));
  }

  function handleDragOver(e: React.DragEvent, docId: string) {
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const midY = rect.top + rect.height / 2;
    setDragOverDocId(docId);
    setDragPosition(e.clientY < midY ? "above" : "below");
  }

  function handleDragLeave() {
    setDragOverDocId(null);
  }

  function handleDrop(e: React.DragEvent, targetIndex: number) {
    e.preventDefault();
    setDragOverDocId(null);
    setDraggingDocId(null);
    try {
      const data = JSON.parse(e.dataTransfer.getData("text/plain"));
      if (data.engagementId !== engagement.id) return;
      const fromIndex: number = data.fromIndex;
      let toIndex = targetIndex;
      if (dragPosition === "below") toIndex += 1;
      // Adjust if moving downward
      if (fromIndex < toIndex) toIndex -= 1;
      onReorderDocs(fromIndex, toIndex);
    } catch {
      // ignore invalid data
    }
  }

  function handleDragEnd() {
    setDraggingDocId(null);
    setDragOverDocId(null);
  }

  return (
    <div style={{ marginBottom: 4 }}>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          padding: "4px 8px",
          borderRadius: 6,
          cursor: "pointer",
          fontSize: 13,
          fontWeight: 500,
          color: "var(--celerex-text-primary)",
        }}
      >
        <span
          onClick={() => setCollapsed(!collapsed)}
          style={{
            marginRight: 4,
            fontSize: 10,
            width: 16,
            textAlign: "center",
            userSelect: "none",
          }}
        >
          {collapsed ? "▸" : "▾"}
        </span>
        {editing ? (
          <InlineInput
            placeholder="Workspace name..."
            defaultValue={engagement.name}
            onSubmit={(val) => {
              onRename(val);
              setEditing(false);
            }}
            onCancel={() => setEditing(false)}
          />
        ) : (
          <span
            onClick={() => setCollapsed(!collapsed)}
            onDoubleClick={(e) => {
              e.stopPropagation();
              setEditing(true);
            }}
            style={{ flex: 1 }}
          >
            {engagement.name}
          </span>
        )}
        <span
          onClick={(e) => {
            e.stopPropagation();
            setCreatingDoc(true);
          }}
          style={{
            fontSize: 16,
            lineHeight: 1,
            opacity: 0.4,
            padding: "0 2px",
            cursor: "pointer",
          }}
          title="New doc"
        >
          +
        </span>
      </div>
      {!collapsed && (
        <div style={{ paddingLeft: 12 }}>
          {engagement.docs.map((doc, index) => (
            <SidebarItem
              key={doc.id}
              label={doc.title || "Untitled"}
              active={doc.id === activeDocId}
              onClick={() => onSelectDoc(doc.id)}
              onDelete={() => onDeleteDoc(doc.id)}
              onDuplicate={() => onDuplicateDoc(doc.id)}
              isFavorited={favorites.includes(doc.id)}
              onToggleFavorite={() => onToggleFavorite(doc.id)}
              draggable
              isDragging={draggingDocId === doc.id}
              dropIndicator={
                dragOverDocId === doc.id ? dragPosition : undefined
              }
              onDragStart={(e) => handleDragStart(e, doc.id, index)}
              onDragOver={(e) => handleDragOver(e, doc.id)}
              onDragLeave={handleDragLeave}
              onDrop={(e) => handleDrop(e, index)}
              onDragEnd={handleDragEnd}
            />
          ))}
          {creatingDoc && (
            <InlineInput
              placeholder="Doc title..."
              onSubmit={(title) => {
                onCreateDoc(title || "Untitled");
                setCreatingDoc(false);
              }}
              onCancel={() => setCreatingDoc(false)}
            />
          )}
        </div>
      )}
    </div>
  );
}

function InlineInput({
  placeholder,
  defaultValue,
  onSubmit,
  onCancel,
}: {
  placeholder: string;
  defaultValue?: string;
  onSubmit: (value: string) => void;
  onCancel: () => void;
}) {
  const ref = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (ref.current) {
      ref.current.focus();
      if (defaultValue) ref.current.select();
    }
  }, []);

  return (
    <input
      ref={ref}
      placeholder={placeholder}
      defaultValue={defaultValue}
      style={{
        display: "block",
        width: "100%",
        padding: "5px 8px",
        fontSize: 13,
        border: "1px solid var(--celerex-accent)",
        borderRadius: 6,
        outline: "none",
        background: "var(--celerex-bg-primary)",
        color: "var(--celerex-text-primary)",
        boxSizing: "border-box",
        margin: "2px 0",
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          const val = (e.target as HTMLInputElement).value.trim();
          if (val) onSubmit(val);
        }
        if (e.key === "Escape") onCancel();
      }}
      onBlur={(e) => {
        const val = e.target.value.trim();
        if (val) onSubmit(val);
        else onCancel();
      }}
    />
  );
}

function SidebarSection({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div style={{ marginBottom: 16 }}>
      <div
        style={{
          fontSize: 11,
          fontWeight: 600,
          textTransform: "uppercase",
          letterSpacing: "0.05em",
          color: "var(--celerex-text-secondary)",
          padding: "4px 8px",
          marginBottom: 2,
        }}
      >
        {title}
      </div>
      {children}
    </div>
  );
}

function SidebarItem({
  label,
  active,
  muted,
  onClick,
  onDelete,
  onDuplicate,
  onUnpin,
  isFavorited,
  onToggleFavorite,
  draggable: isDraggable,
  isDragging,
  dropIndicator,
  onDragStart,
  onDragOver,
  onDragLeave,
  onDrop,
  onDragEnd,
}: {
  label: string;
  active?: boolean;
  muted?: boolean;
  onClick?: () => void;
  onDelete?: () => void;
  onDuplicate?: () => void;
  onUnpin?: () => void;
  isFavorited?: boolean;
  onToggleFavorite?: () => void;
  draggable?: boolean;
  isDragging?: boolean;
  dropIndicator?: "above" | "below";
  onDragStart?: (e: React.DragEvent) => void;
  onDragOver?: (e: React.DragEvent) => void;
  onDragLeave?: () => void;
  onDrop?: (e: React.DragEvent) => void;
  onDragEnd?: () => void;
}) {
  const [hovered, setHovered] = useState(false);

  return (
    <div
      onClick={onClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      draggable={isDraggable}
      onDragStart={onDragStart}
      onDragOver={onDragOver}
      onDragLeave={onDragLeave}
      onDrop={onDrop}
      onDragEnd={onDragEnd}
      style={{
        display: "flex",
        alignItems: "center",
        padding: "5px 8px",
        borderRadius: 6,
        fontSize: 13,
        cursor: isDraggable ? "grab" : onClick ? "pointer" : "default",
        background: active ? "rgba(59, 130, 246, 0.08)" : "transparent",
        color: active
          ? "var(--celerex-accent)"
          : muted
            ? "var(--celerex-text-secondary)"
            : "var(--celerex-text-primary)",
        fontWeight: active ? 500 : 400,
        opacity: isDragging ? 0.5 : 1,
        borderTop: dropIndicator === "above" ? "2px solid #3b82f6" : "2px solid transparent",
        borderBottom: dropIndicator === "below" ? "2px solid #3b82f6" : "2px solid transparent",
        transition: "opacity 0.15s ease",
      }}
    >
      <span style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
        {label}
      </span>
      {onUnpin && hovered && (
        <span
          onClick={(e) => {
            e.stopPropagation();
            onUnpin();
          }}
          style={{
            marginLeft: 4,
            fontSize: 13,
            lineHeight: 1,
            opacity: 0.5,
            cursor: "pointer",
            flexShrink: 0,
          }}
          title="Unpin"
        >
          {"\uD83D\uDCCC"}
        </span>
      )}
      {onToggleFavorite && hovered && (
        <span
          onClick={(e) => {
            e.stopPropagation();
            onToggleFavorite();
          }}
          style={{
            marginLeft: 4,
            fontSize: 13,
            lineHeight: 1,
            opacity: isFavorited ? 0.8 : 0.5,
            cursor: "pointer",
            flexShrink: 0,
          }}
          title={isFavorited ? "Unfavorite" : "Favorite"}
        >
          {isFavorited ? "\u2605" : "\u2606"}
        </span>
      )}
      {onDuplicate && hovered && (
        <span
          onClick={(e) => {
            e.stopPropagation();
            onDuplicate();
          }}
          style={{
            marginLeft: 4,
            fontSize: 14,
            lineHeight: 1,
            opacity: 0.5,
            cursor: "pointer",
            flexShrink: 0,
          }}
          title="Duplicate"
        >
          ⊕
        </span>
      )}
      {onDelete && hovered && (
        <span
          onClick={(e) => {
            e.stopPropagation();
            onDelete();
          }}
          style={{
            marginLeft: 4,
            fontSize: 14,
            lineHeight: 1,
            opacity: 0.5,
            cursor: "pointer",
            flexShrink: 0,
          }}
          title="Delete"
        >
          ×
        </span>
      )}
    </div>
  );
}
