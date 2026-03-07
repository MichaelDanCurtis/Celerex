import { useAtom } from "jotai";
import { scratchpadAtom } from "../state/scratchpadAtom";

interface ScratchpadProps {
  open: boolean;
  onClose: () => void;
}

export function Scratchpad({ open, onClose }: ScratchpadProps) {
  const [content, setContent] = useAtom(scratchpadAtom);

  if (!open) return null;

  return (
    <div
      style={{
        position: "fixed",
        bottom: 60,
        right: 20,
        width: 300,
        height: 250,
        zIndex: 9000,
        display: "flex",
        flexDirection: "column",
        background: "var(--celerex-bg-primary, #fff)",
        border: "1px solid var(--celerex-border, #e5e7eb)",
        borderRadius: 10,
        boxShadow: "0 8px 32px rgba(0,0,0,0.15)",
        overflow: "hidden",
      }}
    >
      {/* Header */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          padding: "8px 12px",
          borderBottom: "1px solid var(--celerex-border, #e5e7eb)",
          background: "var(--celerex-bg-secondary, #f9fafb)",
          flexShrink: 0,
        }}
      >
        <span
          style={{
            fontSize: 13,
            fontWeight: 600,
            color: "var(--celerex-text-primary, #111)",
          }}
        >
          Scratchpad
        </span>
        <button
          onClick={onClose}
          style={{
            border: "none",
            background: "transparent",
            cursor: "pointer",
            fontSize: 16,
            color: "var(--celerex-text-secondary, #888)",
            lineHeight: 1,
            padding: "2px 4px",
            borderRadius: 4,
          }}
          title="Close scratchpad"
        >
          &times;
        </button>
      </div>

      {/* Textarea */}
      <textarea
        value={content}
        onChange={(e) => setContent(e.target.value)}
        style={{
          flex: 1,
          resize: "none",
          border: "none",
          outline: "none",
          padding: "10px 12px",
          fontSize: 13,
          lineHeight: 1.5,
          fontFamily: "inherit",
          background: "transparent",
          color: "var(--celerex-text-primary, #111)",
        }}
        placeholder="Jot down your thoughts..."
      />
    </div>
  );
}
