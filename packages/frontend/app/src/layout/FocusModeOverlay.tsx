import { useAtomValue } from "jotai";
import { activeDocAtom } from "../state/atoms";

interface FocusModeOverlayProps {
  active: boolean;
  onExit: () => void;
}

export function FocusModeOverlay({ active, onExit }: FocusModeOverlayProps) {
  const activeDoc = useAtomValue(activeDocAtom);

  if (!active) return null;

  return (
    <div
      style={{
        height: 32,
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        padding: "0 16px",
        background: "var(--celerex-bg-secondary, rgba(0,0,0,0.03))",
        borderBottom: "1px solid var(--celerex-border, #e5e7eb)",
        flexShrink: 0,
      }}
    >
      <span
        style={{
          fontSize: 13,
          fontWeight: 500,
          color: "var(--celerex-text-primary, #111)",
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
        }}
      >
        {activeDoc?.title ?? "Untitled"}
      </span>
      <button
        onClick={onExit}
        style={{
          padding: "2px 10px",
          borderRadius: 4,
          border: "1px solid var(--celerex-border, #e5e7eb)",
          cursor: "pointer",
          fontSize: 11,
          fontWeight: 500,
          background: "transparent",
          color: "var(--celerex-text-secondary, #888)",
          flexShrink: 0,
        }}
      >
        Exit Focus Mode
      </button>
    </div>
  );
}
