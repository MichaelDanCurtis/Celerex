import { useState } from "react";
import { Sidebar } from "./layout/Sidebar";
import { EditorArea } from "./layout/EditorArea";
import { AIPanel } from "./layout/AIPanel";

export function App() {
  const [mode, setMode] = useState<"page" | "edgeless">("page");
  const [aiPanelOpen, setAiPanelOpen] = useState(true);

  return (
    <div style={{ display: "flex", height: "100vh", width: "100vw" }}>
      <Sidebar />
      <main style={{ flex: 1, display: "flex", flexDirection: "column", overflow: "hidden" }}>
        <Toolbar
          mode={mode}
          onModeChange={setMode}
          aiPanelOpen={aiPanelOpen}
          onToggleAI={() => setAiPanelOpen(!aiPanelOpen)}
        />
        <div style={{ flex: 1, display: "flex", overflow: "hidden" }}>
          <EditorArea mode={mode} />
          {aiPanelOpen && <AIPanel />}
        </div>
      </main>
    </div>
  );
}

function Toolbar({
  mode,
  onModeChange,
  aiPanelOpen,
  onToggleAI,
}: {
  mode: "page" | "edgeless";
  onModeChange: (mode: "page" | "edgeless") => void;
  aiPanelOpen: boolean;
  onToggleAI: () => void;
}) {
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

      <div style={{ flex: 1 }} />

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
