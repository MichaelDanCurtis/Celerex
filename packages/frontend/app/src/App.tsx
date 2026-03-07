import { useState, useEffect, useCallback } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { Sidebar } from "./layout/Sidebar";
import { EditorArea } from "./layout/EditorArea";
import { AIPanel } from "./layout/AIPanel";
import { themeAtom, engagementsAtom, activeDocIdAtom, genId, editorStoreAtom, openDocAtom } from "./state/atoms";
import { exportDocAsMarkdown } from "./utils/export";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";
import { CommandPalette } from "./layout/CommandPalette";
import { useApiSync } from "./hooks/useApiSync";
import { useDocRouter } from "./hooks/useDocRouter";
import { usePersistence } from "./hooks/usePersistence";
import { ToastContainer } from "./layout/ToastContainer";

export function App() {
  const [mode, setMode] = useState<"page" | "edgeless">("page");
  const [aiPanelOpen, setAiPanelOpen] = useState(true);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false);
  const editorStore = useAtomValue(editorStoreAtom);
  const [theme, setTheme] = useAtom(themeAtom);
  const [engagements, setEngagements] = useAtom(engagementsAtom);
  const [, setActiveDocId] = useAtom(activeDocIdAtom);
  const openDoc = useSetAtom(openDocAtom);

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
                { id: docId, title: "Untitled", engagementId: eng.id, createdAt: Date.now() },
              ],
            }
          : e
      )
    );
    openDoc(docId);
  }, [engagements, setEngagements, openDoc]);

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
          if (editorStore) exportDocAsMarkdown(editorStore);
          break;
        case "open-doc":
          if (payload) openDoc(payload);
          break;
      }
    },
    [handleNewDoc, handleToggleTheme, handleToggleAI, handleToggleSidebar, editorStore, openDoc]
  );

  useDocRouter();

  useKeyboardShortcuts({
    onNewDoc: handleNewDoc,
    onToggleAI: handleToggleAI,
    onToggleTheme: handleToggleTheme,
    onToggleSidebar: handleToggleSidebar,
    onCommandPalette: handleCommandPalette,
  });

  return (
    <div style={{ display: "flex", height: "100vh", width: "100vw" }}>
      <Sidebar collapsed={sidebarCollapsed} onToggleCollapse={handleToggleSidebar} />
      <main style={{ flex: 1, display: "flex", flexDirection: "column", overflow: "hidden" }}>
        <Toolbar
          mode={mode}
          onModeChange={setMode}
          aiPanelOpen={aiPanelOpen}
          onToggleAI={handleToggleAI}
          theme={theme}
          onToggleTheme={handleToggleTheme}
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
        <div style={{ flex: 1, display: "flex", overflow: "hidden" }}>
          <EditorArea mode={mode} />
          {aiPanelOpen && <AIPanel />}
        </div>
      </main>
      <ToastContainer />
      <CommandPalette
        open={commandPaletteOpen}
        onClose={() => setCommandPaletteOpen(false)}
        onAction={handlePaletteAction}
      />
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
  syncStatus,
}: {
  mode: "page" | "edgeless";
  onModeChange: (mode: "page" | "edgeless") => void;
  aiPanelOpen: boolean;
  onToggleAI: () => void;
  theme: "light" | "dark";
  onToggleTheme: () => void;
  syncStatus: "syncing" | "synced" | "offline" | "hidden";
}) {
  const editorStore = useAtomValue(editorStoreAtom);

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
