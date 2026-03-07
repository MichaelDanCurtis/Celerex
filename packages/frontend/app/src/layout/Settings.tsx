import { useState } from "react";
import { useAtom } from "jotai";
import { themeAtom } from "../state/atoms";

const ACCENT_COLORS = [
  { name: "Blue", value: "#3b82f6" },
  { name: "Purple", value: "#8b5cf6" },
  { name: "Green", value: "#22c55e" },
  { name: "Orange", value: "#f97316" },
  { name: "Red", value: "#ef4444" },
];

const SHORTCUTS = [
  { keys: "Cmd+N", description: "New document" },
  { keys: "Cmd+B", description: "Toggle sidebar" },
  { keys: "Cmd+K", description: "Command palette" },
  { keys: "Cmd+\\", description: "Toggle AI panel" },
  { keys: "Cmd+Shift+L", description: "Toggle theme" },
];

function ToggleSwitch({
  checked,
  onChange,
}: {
  checked: boolean;
  onChange: (val: boolean) => void;
}) {
  return (
    <button
      onClick={() => onChange(!checked)}
      style={{
        width: 44,
        height: 24,
        borderRadius: 12,
        border: "none",
        cursor: "pointer",
        background: checked ? "var(--celerex-accent)" : "var(--celerex-border)",
        position: "relative",
        transition: "background 0.2s ease",
        flexShrink: 0,
      }}
    >
      <span
        style={{
          position: "absolute",
          top: 2,
          left: checked ? 22 : 2,
          width: 20,
          height: 20,
          borderRadius: "50%",
          background: "#fff",
          transition: "left 0.2s ease",
          boxShadow: "0 1px 3px rgba(0,0,0,0.2)",
        }}
      />
    </button>
  );
}

export function Settings() {
  const [theme, setTheme] = useAtom(themeAtom);
  const [selectedAccent, setSelectedAccent] = useState(0);
  const [defaultMode, setDefaultMode] = useState<"page" | "edgeless">("page");
  const [autoSave, setAutoSave] = useState(true);
  const [spellCheck, setSpellCheck] = useState(true);
  const [aiUrl, setAiUrl] = useState("http://localhost:3001");
  const [aiModel, setAiModel] = useState("claude-sonnet-4-20250514");

  const sectionStyle: React.CSSProperties = {
    marginBottom: 32,
  };

  const sectionTitleStyle: React.CSSProperties = {
    fontSize: 14,
    fontWeight: 600,
    color: "var(--celerex-text-primary)",
    marginBottom: 16,
    paddingBottom: 8,
    borderBottom: "1px solid var(--celerex-border)",
  };

  const rowStyle: React.CSSProperties = {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    padding: "10px 0",
  };

  const labelStyle: React.CSSProperties = {
    fontSize: 13,
    color: "var(--celerex-text-primary)",
    fontWeight: 400,
  };

  const inputStyle: React.CSSProperties = {
    padding: "6px 10px",
    fontSize: 13,
    border: "1px solid var(--celerex-border)",
    borderRadius: 6,
    background: "var(--celerex-bg-primary)",
    color: "var(--celerex-text-primary)",
    outline: "none",
    boxSizing: "border-box" as const,
  };

  return (
    <div
      style={{
        flex: 1,
        overflow: "auto",
        padding: "40px 24px",
        background: "var(--celerex-bg-primary)",
      }}
    >
      <div style={{ maxWidth: 600, margin: "0 auto" }}>
        {/* Header */}
        <div style={{ marginBottom: 36 }}>
          <h1
            style={{
              fontSize: 24,
              fontWeight: 700,
              color: "var(--celerex-text-primary)",
              margin: 0,
            }}
          >
            Settings
          </h1>
          <p
            style={{
              fontSize: 14,
              color: "var(--celerex-text-secondary)",
              margin: "4px 0 0",
            }}
          >
            Configure your workspace
          </p>
        </div>

        {/* Appearance */}
        <div style={sectionStyle}>
          <div style={sectionTitleStyle}>Appearance</div>

          <div style={rowStyle}>
            <span style={labelStyle}>Dark Mode</span>
            <ToggleSwitch
              checked={theme === "dark"}
              onChange={(val) => setTheme(val ? "dark" : "light")}
            />
          </div>

          <div style={{ ...rowStyle, alignItems: "center" }}>
            <span style={labelStyle}>Accent Color</span>
            <div style={{ display: "flex", gap: 8 }}>
              {ACCENT_COLORS.map((color, i) => (
                <button
                  key={color.name}
                  onClick={() => setSelectedAccent(i)}
                  title={color.name}
                  style={{
                    width: 24,
                    height: 24,
                    borderRadius: "50%",
                    border:
                      selectedAccent === i
                        ? "2px solid var(--celerex-text-primary)"
                        : "2px solid transparent",
                    background: color.value,
                    cursor: "pointer",
                    padding: 0,
                    outline: "none",
                    boxShadow:
                      selectedAccent === i
                        ? `0 0 0 2px var(--celerex-bg-primary), 0 0 0 4px ${color.value}`
                        : "none",
                    transition: "box-shadow 0.15s ease",
                  }}
                />
              ))}
            </div>
          </div>
        </div>

        {/* Editor */}
        <div style={sectionStyle}>
          <div style={sectionTitleStyle}>Editor</div>

          <div style={rowStyle}>
            <span style={labelStyle}>Default Mode</span>
            <div style={{ display: "flex", gap: 4 }}>
              {(["page", "edgeless"] as const).map((m) => (
                <button
                  key={m}
                  onClick={() => setDefaultMode(m)}
                  style={{
                    padding: "5px 14px",
                    borderRadius: 6,
                    border: "1px solid var(--celerex-border)",
                    cursor: "pointer",
                    fontSize: 13,
                    fontWeight: 500,
                    background:
                      defaultMode === m ? "var(--celerex-accent)" : "transparent",
                    color:
                      defaultMode === m ? "#fff" : "var(--celerex-text-secondary)",
                    transition: "background 0.15s ease, color 0.15s ease",
                  }}
                >
                  {m.charAt(0).toUpperCase() + m.slice(1)}
                </button>
              ))}
            </div>
          </div>

          <div style={rowStyle}>
            <span style={labelStyle}>Auto-save</span>
            <ToggleSwitch checked={autoSave} onChange={setAutoSave} />
          </div>

          <div style={rowStyle}>
            <span style={labelStyle}>Spell Check</span>
            <ToggleSwitch checked={spellCheck} onChange={setSpellCheck} />
          </div>
        </div>

        {/* AI Assistant */}
        <div style={sectionStyle}>
          <div style={sectionTitleStyle}>AI Assistant</div>

          <div style={{ ...rowStyle, flexDirection: "column", alignItems: "stretch", gap: 6 }}>
            <span style={labelStyle}>AI Backend URL</span>
            <input
              type="text"
              value={aiUrl}
              onChange={(e) => setAiUrl(e.target.value)}
              style={{ ...inputStyle, width: "100%" }}
            />
          </div>

          <div
            style={{
              ...rowStyle,
              flexDirection: "column",
              alignItems: "stretch",
              gap: 6,
              marginTop: 8,
            }}
          >
            <span style={labelStyle}>Model</span>
            <select
              value={aiModel}
              onChange={(e) => setAiModel(e.target.value)}
              style={{ ...inputStyle, width: "100%", height: 34 }}
            >
              <option value="claude-sonnet-4-20250514">claude-sonnet-4-20250514</option>
              <option value="claude-opus-4-20250514">claude-opus-4-20250514</option>
            </select>
          </div>
        </div>

        {/* Keyboard Shortcuts */}
        <div style={sectionStyle}>
          <div style={sectionTitleStyle}>Keyboard Shortcuts</div>

          <table
            style={{
              width: "100%",
              borderCollapse: "collapse",
              fontSize: 13,
            }}
          >
            <thead>
              <tr>
                <th
                  style={{
                    textAlign: "left",
                    padding: "6px 0",
                    fontWeight: 500,
                    color: "var(--celerex-text-secondary)",
                    borderBottom: "1px solid var(--celerex-border)",
                  }}
                >
                  Shortcut
                </th>
                <th
                  style={{
                    textAlign: "left",
                    padding: "6px 0",
                    fontWeight: 500,
                    color: "var(--celerex-text-secondary)",
                    borderBottom: "1px solid var(--celerex-border)",
                  }}
                >
                  Action
                </th>
              </tr>
            </thead>
            <tbody>
              {SHORTCUTS.map((s) => (
                <tr key={s.keys}>
                  <td
                    style={{
                      padding: "8px 0",
                      borderBottom: "1px solid var(--celerex-border)",
                      color: "var(--celerex-text-primary)",
                    }}
                  >
                    <code
                      style={{
                        padding: "2px 8px",
                        borderRadius: 4,
                        background: "var(--celerex-bg-sidebar)",
                        border: "1px solid var(--celerex-border)",
                        fontSize: 12,
                        fontFamily: "monospace",
                      }}
                    >
                      {s.keys}
                    </code>
                  </td>
                  <td
                    style={{
                      padding: "8px 0",
                      borderBottom: "1px solid var(--celerex-border)",
                      color: "var(--celerex-text-secondary)",
                    }}
                  >
                    {s.description}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}
