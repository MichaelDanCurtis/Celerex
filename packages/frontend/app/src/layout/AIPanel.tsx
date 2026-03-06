import { useState } from "react";

export function AIPanel() {
  const [input, setInput] = useState("");

  return (
    <aside
      style={{
        width: "var(--celerex-ai-panel-width)",
        height: "100%",
        borderLeft: "1px solid var(--celerex-border)",
        display: "flex",
        flexDirection: "column",
        background: "var(--celerex-bg-primary)",
      }}
    >
      {/* Header */}
      <div
        style={{
          padding: "12px 16px",
          borderBottom: "1px solid var(--celerex-border)",
          fontWeight: 600,
          fontSize: 14,
          display: "flex",
          alignItems: "center",
          gap: 8,
        }}
      >
        <span style={{ fontSize: 16 }}>AI</span>
        <span style={{ color: "var(--celerex-text-secondary)", fontWeight: 400 }}>
          Co-Pilot
        </span>
      </div>

      {/* Messages Area */}
      <div
        style={{
          flex: 1,
          padding: 16,
          overflow: "auto",
          display: "flex",
          flexDirection: "column",
          gap: 12,
        }}
      >
        <div
          style={{
            background: "var(--celerex-bg-secondary)",
            borderRadius: 8,
            padding: 12,
            fontSize: 13,
            lineHeight: 1.5,
          }}
        >
          Welcome to Celerex. I'm your AI co-pilot with full context of your
          workspace — documents, diagrams, agent outputs, and more.
          <br /><br />
          How can I help?
        </div>
      </div>

      {/* Input */}
      <div
        style={{
          padding: 12,
          borderTop: "1px solid var(--celerex-border)",
        }}
      >
        <div
          style={{
            display: "flex",
            gap: 8,
            alignItems: "flex-end",
          }}
        >
          <textarea
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder="Ask anything about your workspace..."
            rows={2}
            style={{
              flex: 1,
              resize: "none",
              border: "1px solid var(--celerex-border)",
              borderRadius: 8,
              padding: "8px 12px",
              fontSize: 13,
              fontFamily: "inherit",
              outline: "none",
            }}
          />
          <button
            style={{
              padding: "8px 16px",
              borderRadius: 8,
              border: "none",
              background: "var(--celerex-accent)",
              color: "#fff",
              cursor: "pointer",
              fontSize: 13,
              fontWeight: 500,
              whiteSpace: "nowrap",
            }}
          >
            Send
          </button>
        </div>
      </div>
    </aside>
  );
}
