import { useState, useEffect, useRef, useMemo } from "react";
import { useAtomValue } from "jotai";
import { engagementsAtom } from "../state/atoms";

interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
  onAction: (action: string, payload?: any) => void;
}

interface PaletteItem {
  id: string;
  label: string;
  icon: string;
  action: string;
  payload?: any;
}

function fuzzyMatch(query: string, text: string): boolean {
  const q = query.toLowerCase();
  const t = text.toLowerCase();
  let qi = 0;
  for (let ti = 0; ti < t.length && qi < q.length; ti++) {
    if (t[ti] === q[qi]) qi++;
  }
  return qi === q.length;
}

export function CommandPalette({ open, onClose, onAction }: CommandPaletteProps) {
  const engagements = useAtomValue(engagementsAtom);
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  // Build items list: actions + all docs
  const allItems = useMemo<PaletteItem[]>(() => {
    const actions: PaletteItem[] = [
      { id: "action:new-doc", label: "New Document", icon: "\u26A1", action: "new-doc" },
      { id: "action:toggle-dark-mode", label: "Toggle Dark Mode", icon: "\u26A1", action: "toggle-theme" },
      { id: "action:toggle-ai-panel", label: "Toggle AI Panel", icon: "\u26A1", action: "toggle-ai" },
      { id: "action:toggle-sidebar", label: "Toggle Sidebar", icon: "\u26A1", action: "toggle-sidebar" },
      { id: "action:export-markdown", label: "Export as Markdown", icon: "\u26A1", action: "export-markdown" },
      { id: "action:import-markdown", label: "Import Markdown", icon: "\u26A1", action: "import-markdown" },
      { id: "action:new-from-template", label: "New from Template...", icon: "\u26A1", action: "new-from-template" },
    ];

    const docs: PaletteItem[] = engagements.flatMap((eng) =>
      eng.docs.map((doc) => ({
        id: `doc:${doc.id}`,
        label: `${doc.title}  \u2014  ${eng.name}`,
        icon: "\uD83D\uDCC4",
        action: "open-doc",
        payload: doc.id,
      }))
    );

    return [...actions, ...docs];
  }, [engagements]);

  const filtered = useMemo(() => {
    if (!query.trim()) return allItems;
    return allItems.filter((item) => fuzzyMatch(query, item.label));
  }, [query, allItems]);

  // Reset state when opened/closed
  useEffect(() => {
    if (open) {
      setQuery("");
      setSelectedIndex(0);
      // Focus input on next tick
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [open]);

  // Clamp selected index when filtered list changes
  useEffect(() => {
    setSelectedIndex((prev) => Math.min(prev, Math.max(0, filtered.length - 1)));
  }, [filtered.length]);

  // Scroll selected item into view
  useEffect(() => {
    if (!listRef.current) return;
    const el = listRef.current.children[selectedIndex] as HTMLElement | undefined;
    el?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex((i) => Math.min(i + 1, filtered.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const item = filtered[selectedIndex];
      if (item) {
        onAction(item.action, item.payload);
        onClose();
      }
    } else if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }

  if (!open) return null;

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 9999,
        display: "flex",
        alignItems: "flex-start",
        justifyContent: "center",
        paddingTop: 120,
        background: "rgba(0, 0, 0, 0.4)",
      }}
      onClick={onClose}
    >
      <div
        style={{
          width: "100%",
          maxWidth: 500,
          background: "var(--celerex-bg-primary, #fff)",
          borderRadius: 12,
          boxShadow: "0 16px 48px rgba(0,0,0,0.2)",
          overflow: "hidden",
          display: "flex",
          flexDirection: "column",
          maxHeight: "min(420px, 60vh)",
        }}
        onClick={(e) => e.stopPropagation()}
        onKeyDown={handleKeyDown}
      >
        {/* Search input */}
        <div
          style={{
            padding: "12px 16px",
            borderBottom: "1px solid var(--celerex-border, #e5e7eb)",
          }}
        >
          <input
            ref={inputRef}
            type="text"
            placeholder="Type a command or search docs..."
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setSelectedIndex(0);
            }}
            style={{
              width: "100%",
              padding: "8px 0",
              border: "none",
              outline: "none",
              fontSize: 15,
              background: "transparent",
              color: "var(--celerex-text-primary, #111)",
            }}
          />
        </div>

        {/* Results list */}
        <div
          ref={listRef}
          style={{
            overflowY: "auto",
            padding: "4px 0",
          }}
        >
          {filtered.length === 0 && (
            <div
              style={{
                padding: "16px",
                textAlign: "center",
                color: "var(--celerex-text-secondary, #888)",
                fontSize: 13,
              }}
            >
              No results found
            </div>
          )}
          {filtered.map((item, i) => (
            <div
              key={item.id}
              onClick={() => {
                onAction(item.action, item.payload);
                onClose();
              }}
              onMouseEnter={() => setSelectedIndex(i)}
              style={{
                padding: "8px 16px",
                display: "flex",
                alignItems: "center",
                gap: 10,
                cursor: "pointer",
                fontSize: 14,
                color: "var(--celerex-text-primary, #111)",
                background:
                  i === selectedIndex
                    ? "var(--celerex-bg-hover, rgba(0,0,0,0.06))"
                    : "transparent",
              }}
            >
              <span style={{ fontSize: 15, flexShrink: 0 }}>{item.icon}</span>
              <span
                style={{
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {item.label}
              </span>
            </div>
          ))}
        </div>

        {/* Footer hint */}
        <div
          style={{
            padding: "8px 16px",
            borderTop: "1px solid var(--celerex-border, #e5e7eb)",
            fontSize: 11,
            color: "var(--celerex-text-secondary, #888)",
            display: "flex",
            gap: 12,
          }}
        >
          <span>{"\u2191\u2193"} navigate</span>
          <span>{"\u21B5"} select</span>
          <span>esc close</span>
        </div>
      </div>
    </div>
  );
}
