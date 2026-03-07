import { useEffect } from "react";

interface ShortcutsDialogProps {
  open: boolean;
  onClose: () => void;
}

interface ShortcutEntry {
  keys: string[];
  description: string;
}

interface ShortcutCategory {
  title: string;
  shortcuts: ShortcutEntry[];
}

const categories: ShortcutCategory[] = [
  {
    title: "General",
    shortcuts: [
      { keys: ["\u2318", "K"], description: "Command Palette" },
      { keys: ["\u2318", "N"], description: "New Document" },
      { keys: ["\u2318", "\u21E7", "N"], description: "New from Template" },
    ],
  },
  {
    title: "Editor",
    shortcuts: [
      { keys: ["\u2318", "\\"], description: "Toggle AI Panel" },
      { keys: ["\u2318", "B"], description: "Toggle Sidebar" },
      { keys: ["\u2318", "\u21E7", "L"], description: "Toggle Dark Mode" },
    ],
  },
  {
    title: "Navigation",
    shortcuts: [
      { keys: ["\u2191", "\u2193"], description: "Navigate palette" },
      { keys: ["\u21B5"], description: "Select" },
      { keys: ["Esc"], description: "Close" },
    ],
  },
];

const kbdStyle: React.CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
  minWidth: 24,
  height: 24,
  padding: "0 6px",
  borderRadius: 5,
  border: "1px solid var(--celerex-border, #d1d5db)",
  background: "var(--celerex-bg-primary, #fff)",
  fontFamily: "ui-monospace, 'SF Mono', Monaco, 'Cascadia Code', monospace",
  fontSize: 12,
  fontWeight: 600,
  color: "var(--celerex-text-primary, #111)",
  lineHeight: 1,
  boxShadow: "0 1px 0 var(--celerex-border, #d1d5db)",
};

export function ShortcutsDialog({ open, onClose }: ShortcutsDialogProps) {
  useEffect(() => {
    if (!open) return;
    function handleKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    }
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 9999,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "rgba(0, 0, 0, 0.4)",
      }}
      onClick={onClose}
    >
      <div
        style={{
          width: "100%",
          maxWidth: 520,
          background: "var(--celerex-bg-primary, #fff)",
          borderRadius: 12,
          boxShadow: "0 16px 48px rgba(0,0,0,0.2)",
          overflow: "hidden",
          display: "flex",
          flexDirection: "column",
          maxHeight: "min(560px, 80vh)",
        }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div
          style={{
            padding: "16px 20px",
            borderBottom: "1px solid var(--celerex-border, #e5e7eb)",
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
          }}
        >
          <h2
            style={{
              margin: 0,
              fontSize: 16,
              fontWeight: 600,
              color: "var(--celerex-text-primary, #111)",
            }}
          >
            Keyboard Shortcuts
          </h2>
          <button
            onClick={onClose}
            style={{
              border: "none",
              background: "none",
              cursor: "pointer",
              fontSize: 18,
              color: "var(--celerex-text-secondary, #888)",
              lineHeight: 1,
              padding: 4,
            }}
          >
            {"\u2715"}
          </button>
        </div>

        {/* Content */}
        <div style={{ overflowY: "auto", padding: "12px 20px 20px" }}>
          {categories.map((cat) => (
            <div key={cat.title} style={{ marginBottom: 20 }}>
              <h3
                style={{
                  margin: "0 0 8px",
                  fontSize: 11,
                  fontWeight: 700,
                  textTransform: "uppercase",
                  letterSpacing: "0.05em",
                  color: "var(--celerex-text-secondary, #888)",
                }}
              >
                {cat.title}
              </h3>
              <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                {cat.shortcuts.map((s) => (
                  <div
                    key={s.description}
                    style={{
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "space-between",
                      padding: "6px 0",
                    }}
                  >
                    <span
                      style={{
                        fontSize: 13,
                        color: "var(--celerex-text-primary, #111)",
                      }}
                    >
                      {s.description}
                    </span>
                    <span style={{ display: "flex", gap: 4 }}>
                      {s.keys.map((k, i) => (
                        <kbd key={i} style={kbdStyle}>
                          {k}
                        </kbd>
                      ))}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>

        {/* Footer */}
        <div
          style={{
            padding: "10px 20px",
            borderTop: "1px solid var(--celerex-border, #e5e7eb)",
            fontSize: 11,
            color: "var(--celerex-text-secondary, #888)",
            textAlign: "center",
          }}
        >
          Press <kbd style={{ ...kbdStyle, height: 18, minWidth: 18, fontSize: 10 }}>{"\u2318"}</kbd>{" "}
          <kbd style={{ ...kbdStyle, height: 18, minWidth: 18, fontSize: 10 }}>/</kbd> to toggle this dialog
        </div>
      </div>
    </div>
  );
}
