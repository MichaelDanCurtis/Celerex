import { useState, useEffect, useCallback, useRef } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { Sidebar } from "./layout/Sidebar";
import { EditorArea } from "./layout/EditorArea";
import { AIPanel } from "./layout/AIPanel";
import { themeAtom, engagementsAtom, activeDocIdAtom, genId, editorStoreAtom, openDocAtom, activeViewAtom } from "./state/atoms";
import { exportDocAsMarkdown, printDocument } from "./utils/export";
import { importMarkdownFile } from "./utils/import";
import { addToastAtom } from "./state/toastAtom";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";
import { CommandPalette } from "./layout/CommandPalette";
import { TemplateDialog } from "./layout/TemplateDialog";
import { type DocTemplate } from "./templates/templates";
import { useApiSync } from "./hooks/useApiSync";
import { useDocRouter } from "./hooks/useDocRouter";
import { usePersistence } from "./hooks/usePersistence";
import { ToastContainer } from "./layout/ToastContainer";
import { AgentDashboard } from "./layout/AgentDashboard";
import { PipelineEditor } from "./layout/PipelineEditor";
import { ToolsManager } from "./layout/ToolsManager";
import { TaskQueue } from "./layout/TaskQueue";
import { PluginManager } from "./layout/PluginManager";
import { Settings } from "./layout/Settings";
import { Breadcrumb } from "./layout/Breadcrumb";
import { unreadCountAtom } from "./state/notificationAtom";
import { NotificationPanel } from "./layout/NotificationPanel";
import { VersionHistory } from "./layout/VersionHistory";
import { DocOutline } from "./layout/DocOutline";
import { ShortcutsDialog } from "./layout/ShortcutsDialog";
import { ShareDialog } from "./layout/ShareDialog";
import { FocusModeOverlay } from "./layout/FocusModeOverlay";
import { SplitView } from "./layout/SplitView";
import { ActivityLog } from "./layout/ActivityLog";
import { Scratchpad } from "./layout/Scratchpad";
import { PresenceIndicator } from "./layout/PresenceIndicator";
import { TrashBin } from "./layout/TrashBin";
import { TimelineView } from "./layout/TimelineView";
import { FindReplace } from "./layout/FindReplace";
import { ExportDialog } from "./layout/ExportDialog";
import { OnboardingDialog } from "./layout/OnboardingDialog";

export function App() {
  const [mode, setMode] = useState<"page" | "edgeless">("page");
  const [aiPanelOpen, setAiPanelOpen] = useState(true);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false);
  const [templateDialogOpen, setTemplateDialogOpen] = useState(false);
  const [versionHistoryOpen, setVersionHistoryOpen] = useState(false);
  const [outlineOpen, setOutlineOpen] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const [focusModeActive, setFocusModeActive] = useState(false);
  const [splitViewOpen, setSplitViewOpen] = useState(false);
  const [shareDialogOpen, setShareDialogOpen] = useState(false);
  const [activityLogOpen, setActivityLogOpen] = useState(false);
  const [scratchpadOpen, setScratchpadOpen] = useState(false);
  const [trashOpen, setTrashOpen] = useState(false);
  const [findReplaceOpen, setFindReplaceOpen] = useState(false);
  const [exportDialogOpen, setExportDialogOpen] = useState(false);
  const [showOnboarding, setShowOnboarding] = useState(() => !localStorage.getItem("celerex-onboarded"));
  const preFocusStateRef = useRef<{ sidebarCollapsed: boolean; aiPanelOpen: boolean } | null>(null);
  const editorStore = useAtomValue(editorStoreAtom);
  const [theme, setTheme] = useAtom(themeAtom);
  const [engagements, setEngagements] = useAtom(engagementsAtom);
  const [, setActiveDocId] = useAtom(activeDocIdAtom);
  const openDoc = useSetAtom(openDocAtom);
  const addToast = useSetAtom(addToastAtom);
  const activeView = useAtomValue(activeViewAtom);

  usePersistence();
  const { synced, error } = useApiSync();
  const [showSynced, setShowSynced] = useState(true);

  useEffect(() => {
    if (synced && !error) {
      const timer = setTimeout(() => setShowSynced(false), 2000);
      return () => clearTimeout(timer);
    }
  }, [synced, error]);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  const handleNewDoc = useCallback(() => {
    setTemplateDialogOpen(true);
  }, []);

  const handleTemplateSelect = useCallback(
    (template: DocTemplate) => {
      const eng = engagements[0];
      if (!eng) return;
      const docId = genId("doc");
      setEngagements((prev) =>
        prev.map((e) =>
          e.id === eng.id
            ? {
                ...e,
                docs: [
                  ...e.docs,
                  { id: docId, title: template.titlePrefix, engagementId: eng.id, createdAt: Date.now() },
                ],
              }
            : e
        )
      );
      openDoc(docId);
    },
    [engagements, setEngagements, openDoc]
  );

  const handleOpenTemplateDialog = useCallback(() => {
    setTemplateDialogOpen(true);
  }, []);

  const handleImportMarkdown = useCallback(async () => {
    const result = await importMarkdownFile();
    if (!result) return;
    const eng = engagements[0];
    if (!eng) return;
    const docId = genId("doc");
    setEngagements((prev) =>
      prev.map((e) =>
        e.id === eng.id
          ? {
              ...e,
              docs: [
                ...e.docs,
                { id: docId, title: result.title, engagementId: eng.id, createdAt: Date.now() },
              ],
            }
          : e
      )
    );
    openDoc(docId);
    addToast({ message: `Imported: ${result.title}`, type: "success" });
  }, [engagements, setEngagements, openDoc, addToast]);

  const handleToggleAI = useCallback(() => {
    setAiPanelOpen((prev) => !prev);
  }, []);

  const handleToggleTheme = useCallback(() => {
    setTheme((prev) => (prev === "light" ? "dark" : "light"));
  }, [setTheme]);

  const handleToggleSidebar = useCallback(() => {
    setSidebarCollapsed((prev) => !prev);
  }, []);

  const handleCommandPalette = useCallback(() => {
    setCommandPaletteOpen((prev) => !prev);
  }, []);

  const handleToggleFocusMode = useCallback(() => {
    setFocusModeActive((prev) => {
      if (!prev) {
        // Entering focus mode: save current state
        preFocusStateRef.current = { sidebarCollapsed, aiPanelOpen };
        setSidebarCollapsed(true);
        setAiPanelOpen(false);
      } else {
        // Exiting focus mode: restore previous state
        if (preFocusStateRef.current) {
          setSidebarCollapsed(preFocusStateRef.current.sidebarCollapsed);
          setAiPanelOpen(preFocusStateRef.current.aiPanelOpen);
          preFocusStateRef.current = null;
        }
      }
      return !prev;
    });
  }, [sidebarCollapsed, aiPanelOpen]);

  const handlePaletteAction = useCallback(
    (action: string, payload?: any) => {
      switch (action) {
        case "new-doc":
          handleNewDoc();
          break;
        case "toggle-theme":
          handleToggleTheme();
          break;
        case "toggle-ai":
          handleToggleAI();
          break;
        case "toggle-sidebar":
          handleToggleSidebar();
          break;
        case "export-markdown":
          setExportDialogOpen(true);
          break;
        case "print-document":
          if (editorStore) printDocument(editorStore);
          break;
        case "import-markdown":
          handleImportMarkdown();
          break;
        case "new-from-template":
          handleOpenTemplateDialog();
          break;
        case "open-doc":
          if (payload) openDoc(payload);
          break;
        case "version-history":
          setVersionHistoryOpen((prev) => !prev);
          break;
        case "toggle-outline":
          setOutlineOpen((prev) => !prev);
          break;
        case "keyboard-shortcuts":
          setShortcutsOpen(true);
          break;
        case "focus-mode":
          handleToggleFocusMode();
          break;
        case "split-view":
          setSplitViewOpen((prev) => !prev);
          break;
        case "share-document":
          setShareDialogOpen(true);
          break;
        case "activity-log":
          setActivityLogOpen((prev) => !prev);
          break;
        case "scratchpad":
          setScratchpadOpen((prev) => !prev);
          break;
        case "trash":
          setTrashOpen((prev) => !prev);
          break;
        case "find-replace":
          setFindReplaceOpen((prev) => !prev);
          break;
      }
    },
    [handleNewDoc, handleToggleTheme, handleToggleAI, handleToggleSidebar, editorStore, openDoc, handleImportMarkdown, handleOpenTemplateDialog, handleToggleFocusMode]
  );

  useDocRouter();

  useKeyboardShortcuts({
    onNewDoc: handleNewDoc,
    onToggleAI: handleToggleAI,
    onToggleTheme: handleToggleTheme,
    onToggleSidebar: handleToggleSidebar,
    onCommandPalette: handleCommandPalette,
    onNewFromTemplate: handleOpenTemplateDialog,
    onShortcutsHelp: useCallback(() => setShortcutsOpen((prev) => !prev), []),
    onFocusMode: handleToggleFocusMode,
    onFind: useCallback(() => setFindReplaceOpen((prev) => !prev), []),
  });

  return (
    <div style={{ display: "flex", height: "100vh", width: "100vw" }}>
      {!focusModeActive && <Sidebar collapsed={sidebarCollapsed} onToggleCollapse={handleToggleSidebar} />}
      <main style={{ flex: 1, display: "flex", flexDirection: "column", overflow: "hidden" }}>
        {focusModeActive ? (
          <FocusModeOverlay active={focusModeActive} onExit={handleToggleFocusMode} />
        ) : (
          <>
            <Toolbar
              mode={mode}
              onModeChange={setMode}
              aiPanelOpen={aiPanelOpen}
              onToggleAI={handleToggleAI}
              theme={theme}
              onToggleTheme={handleToggleTheme}
              onImportMarkdown={handleImportMarkdown}
              versionHistoryOpen={versionHistoryOpen}
              onToggleVersionHistory={() => setVersionHistoryOpen((prev) => !prev)}
              outlineOpen={outlineOpen}
              onToggleOutline={() => setOutlineOpen((prev) => !prev)}
              activityLogOpen={activityLogOpen}
              onToggleActivityLog={() => setActivityLogOpen((prev) => !prev)}
              syncStatus={
                error
                  ? "offline"
                  : synced
                    ? showSynced
                      ? "synced"
                      : "hidden"
                    : "syncing"
              }
            />
            <Breadcrumb />
          </>
        )}
        <FindReplace open={findReplaceOpen} onClose={() => setFindReplaceOpen(false)} />
        <div style={{ flex: 1, display: "flex", overflow: "hidden" }}>
          {activeView === "settings" ? (
            <Settings />
          ) : activeView === "agent-dashboard" ? (
            <AgentDashboard />
          ) : activeView === "pipeline-editor" ? (
            <PipelineEditor />
          ) : activeView === "tools-manager" ? (
            <ToolsManager />
          ) : activeView === "task-queue" ? (
            <TaskQueue />
          ) : activeView === "timeline" ? (
            <TimelineView />
          ) : activeView === "plugin-manager" ? (
            <PluginManager />
          ) : (
            <div style={{ flex: 1, display: "flex", overflow: "hidden" }}>
              <EditorArea mode={mode} activeView={activeView} />
              {splitViewOpen && <SplitView onClose={() => setSplitViewOpen(false)} />}
            </div>
          )}
          {outlineOpen && <DocOutline onClose={() => setOutlineOpen(false)} />}
          {versionHistoryOpen && <VersionHistory onClose={() => setVersionHistoryOpen(false)} />}
          {activityLogOpen && <ActivityLog onClose={() => setActivityLogOpen(false)} />}
          {trashOpen && <TrashBin onClose={() => setTrashOpen(false)} />}
          {!focusModeActive && aiPanelOpen && <AIPanel />}
        </div>
      </main>
      <ToastContainer />
      <CommandPalette
        open={commandPaletteOpen}
        onClose={() => setCommandPaletteOpen(false)}
        onAction={handlePaletteAction}
      />
      <TemplateDialog
        open={templateDialogOpen}
        onClose={() => setTemplateDialogOpen(false)}
        onSelect={handleTemplateSelect}
      />
      <ShortcutsDialog
        open={shortcutsOpen}
        onClose={() => setShortcutsOpen(false)}
      />
      <ShareDialog
        open={shareDialogOpen}
        onClose={() => setShareDialogOpen(false)}
      />
      <Scratchpad
        open={scratchpadOpen}
        onClose={() => setScratchpadOpen(false)}
      />
      <ExportDialog
        open={exportDialogOpen}
        onClose={() => setExportDialogOpen(false)}
      />
      {showOnboarding && (
        <OnboardingDialog
          onComplete={() => {
            localStorage.setItem("celerex-onboarded", "true");
            setShowOnboarding(false);
          }}
        />
      )}
    </div>
  );
}

function Toolbar({
  mode,
  onModeChange,
  aiPanelOpen,
  onToggleAI,
  theme,
  onToggleTheme,
  onImportMarkdown,
  versionHistoryOpen,
  onToggleVersionHistory,
  outlineOpen,
  onToggleOutline,
  activityLogOpen,
  onToggleActivityLog,
  syncStatus,
}: {
  mode: "page" | "edgeless";
  onModeChange: (mode: "page" | "edgeless") => void;
  aiPanelOpen: boolean;
  onToggleAI: () => void;
  theme: "light" | "dark";
  onToggleTheme: () => void;
  onImportMarkdown: () => void;
  versionHistoryOpen: boolean;
  onToggleVersionHistory: () => void;
  outlineOpen: boolean;
  onToggleOutline: () => void;
  activityLogOpen: boolean;
  onToggleActivityLog: () => void;
  syncStatus: "syncing" | "synced" | "offline" | "hidden";
}) {
  const editorStore = useAtomValue(editorStoreAtom);
  const unreadCount = useAtomValue(unreadCountAtom);
  const activeDocId = useAtomValue(activeDocIdAtom);
  const [notifPanelOpen, setNotifPanelOpen] = useState(false);

  return (
    <div
      style={{
        height: 48,
        borderBottom: "1px solid var(--celerex-border)",
        display: "flex",
        alignItems: "center",
        padding: "0 16px",
        gap: 8,
        background: "var(--celerex-bg-primary)",
      }}
    >
      <div style={{ display: "flex", gap: 4 }}>
        <button
          onClick={() => onModeChange("page")}
          style={{
            padding: "6px 12px",
            borderRadius: 6,
            border: "none",
            cursor: "pointer",
            fontSize: 13,
            fontWeight: 500,
            background: mode === "page" ? "var(--celerex-accent)" : "transparent",
            color: mode === "page" ? "#fff" : "var(--celerex-text-secondary)",
          }}
        >
          Page
        </button>
        <button
          onClick={() => onModeChange("edgeless")}
          style={{
            padding: "6px 12px",
            borderRadius: 6,
            border: "none",
            cursor: "pointer",
            fontSize: 13,
            fontWeight: 500,
            background: mode === "edgeless" ? "var(--celerex-accent)" : "transparent",
            color: mode === "edgeless" ? "#fff" : "var(--celerex-text-secondary)",
          }}
        >
          Edgeless
        </button>
      </div>

      {/* Notification bell */}
      <div style={{ position: "relative" }}>
        <button
          onClick={() => setNotifPanelOpen((prev) => !prev)}
          style={{
            padding: "6px 10px",
            borderRadius: 6,
            border: "1px solid var(--celerex-border)",
            cursor: "pointer",
            fontSize: 15,
            background: notifPanelOpen ? "var(--celerex-accent)" : "transparent",
            color: notifPanelOpen ? "#fff" : "var(--celerex-text-secondary)",
            position: "relative",
            lineHeight: 1,
          }}
          title="Notifications"
        >
          {"\uD83D\uDD14"}
          {unreadCount > 0 && (
            <span
              style={{
                position: "absolute",
                top: 2,
                right: 2,
                minWidth: 16,
                height: 16,
                borderRadius: "50%",
                background: "#ef4444",
                color: "#fff",
                fontSize: 10,
                fontWeight: 700,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                lineHeight: 1,
                padding: "0 3px",
              }}
            >
              {unreadCount}
            </span>
          )}
        </button>
        {notifPanelOpen && (
          <NotificationPanel onClose={() => setNotifPanelOpen(false)} />
        )}
      </div>

      {syncStatus !== "hidden" && (
        <span
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 4,
            fontSize: 11,
            color:
              syncStatus === "offline"
                ? "#d97706"
                : syncStatus === "synced"
                  ? "#22c55e"
                  : "#9ca3af",
            opacity: syncStatus === "synced" ? 0.8 : 0.7,
            transition: "opacity 0.3s ease",
            userSelect: "none",
          }}
        >
          <span
            style={{
              width: 6,
              height: 6,
              borderRadius: "50%",
              background:
                syncStatus === "offline"
                  ? "#d97706"
                  : syncStatus === "synced"
                    ? "#22c55e"
                    : "#9ca3af",
            }}
          />
          {syncStatus === "syncing"
            ? "Syncing\u2026"
            : syncStatus === "synced"
              ? "Synced"
              : "Offline"}
        </span>
      )}

      <PresenceIndicator docId={activeDocId} />

      <div style={{ flex: 1 }} />

      <button
        onClick={onToggleTheme}
        style={{
          padding: "6px 12px",
          borderRadius: 6,
          border: "1px solid var(--celerex-border)",
          cursor: "pointer",
          fontSize: 13,
          fontWeight: 500,
          background: "transparent",
          color: "var(--celerex-text-secondary)",
        }}
        title={`Switch to ${theme === "light" ? "dark" : "light"} mode`}
      >
        {theme === "light" ? "\u263D Dark" : "\u2600 Light"}
      </button>

      <button
        onClick={onToggleVersionHistory}
        style={{
          padding: "6px 12px",
          borderRadius: 6,
          border: "1px solid var(--celerex-border)",
          cursor: "pointer",
          fontSize: 13,
          fontWeight: 500,
          background: versionHistoryOpen ? "var(--celerex-accent)" : "transparent",
          color: versionHistoryOpen ? "#fff" : "var(--celerex-text-secondary)",
        }}
        title="Toggle version history"
      >
        History
      </button>

      <button
        onClick={onToggleOutline}
        style={{
          padding: "6px 12px",
          borderRadius: 6,
          border: "1px solid var(--celerex-border)",
          cursor: "pointer",
          fontSize: 13,
          fontWeight: 500,
          background: outlineOpen ? "var(--celerex-accent)" : "transparent",
          color: outlineOpen ? "#fff" : "var(--celerex-text-secondary)",
        }}
        title="Toggle document outline"
      >
        Outline
      </button>

      <button
        onClick={onToggleActivityLog}
        style={{
          padding: "6px 12px",
          borderRadius: 6,
          border: "1px solid var(--celerex-border)",
          cursor: "pointer",
          fontSize: 13,
          fontWeight: 500,
          background: activityLogOpen ? "var(--celerex-accent)" : "transparent",
          color: activityLogOpen ? "#fff" : "var(--celerex-text-secondary)",
        }}
        title="Toggle activity log"
      >
        Activity
      </button>

      <button
        onClick={() => editorStore && exportDocAsMarkdown(editorStore)}
        disabled={!editorStore}
        style={{
          padding: "6px 12px",
          borderRadius: 6,
          border: "1px solid var(--celerex-border)",
          cursor: editorStore ? "pointer" : "not-allowed",
          fontSize: 13,
          fontWeight: 500,
          background: "transparent",
          color: "var(--celerex-text-secondary)",
          opacity: editorStore ? 1 : 0.5,
        }}
        title="Export document as Markdown"
      >
        Export
      </button>

      <button
        onClick={() => editorStore && printDocument(editorStore)}
        disabled={!editorStore}
        style={{
          padding: "6px 12px",
          borderRadius: 6,
          border: "1px solid var(--celerex-border)",
          cursor: editorStore ? "pointer" : "not-allowed",
          fontSize: 13,
          fontWeight: 500,
          background: "transparent",
          color: "var(--celerex-text-secondary)",
          opacity: editorStore ? 1 : 0.5,
        }}
        title="Print document / Save as PDF"
      >
        Print
      </button>

      <button
        onClick={onImportMarkdown}
        style={{
          padding: "6px 12px",
          borderRadius: 6,
          border: "1px solid var(--celerex-border)",
          cursor: "pointer",
          fontSize: 13,
          fontWeight: 500,
          background: "transparent",
          color: "var(--celerex-text-secondary)",
        }}
        title="Import a Markdown file as a new document"
      >
        Import
      </button>

      <button
        onClick={onToggleAI}
        style={{
          padding: "6px 12px",
          borderRadius: 6,
          border: "1px solid var(--celerex-border)",
          cursor: "pointer",
          fontSize: 13,
          fontWeight: 500,
          background: aiPanelOpen ? "var(--celerex-accent)" : "transparent",
          color: aiPanelOpen ? "#fff" : "var(--celerex-text-secondary)",
        }}
      >
        AI Panel
      </button>
    </div>
  );
}
