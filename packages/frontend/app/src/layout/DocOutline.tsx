import { useState, useEffect } from "react";
import { useAtomValue } from "jotai";
import { editorStoreAtom } from "../state/atoms";

interface DocOutlineProps {
  onClose: () => void;
}

interface HeadingItem {
  level: number;
  text: string;
}

const HEADING_TYPES = new Set(["h1", "h2", "h3", "h4"]);

const INDENT_MAP: Record<number, number> = {
  1: 0,
  2: 16,
  3: 32,
  4: 48,
};

function extractHeadings(store: any): HeadingItem[] {
  if (!store?.root?.children) return [];

  const headings: HeadingItem[] = [];
  for (const child of store.root.children) {
    if (child.flavour !== "affine:paragraph") continue;
    const type = child.yBlock?.get("prop:type");
    if (!type || !HEADING_TYPES.has(type)) continue;
    const text = child.yBlock?.get("prop:text")?.toString() ?? "";
    if (!text) continue;
    const level = parseInt(type.charAt(1), 10);
    headings.push({ level, text });
  }

  return headings;
}

export function DocOutline({ onClose }: DocOutlineProps) {
  const store = useAtomValue(editorStoreAtom);
  const [headings, setHeadings] = useState<HeadingItem[]>([]);

  useEffect(() => {
    if (!store) return;

    setHeadings(extractHeadings(store));

    const id = setInterval(() => {
      setHeadings(extractHeadings(store));
    }, 2000);

    return () => clearInterval(id);
  }, [store]);

  return (
    <div
      style={{
        width: 240,
        minWidth: 240,
        borderLeft: "1px solid var(--celerex-border)",
        background: "var(--celerex-bg-primary)",
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
      }}
    >
      {/* Header */}
      <div
        style={{
          padding: "12px 16px",
          borderBottom: "1px solid var(--celerex-border)",
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          flexShrink: 0,
        }}
      >
        <span
          style={{
            fontSize: 14,
            fontWeight: 600,
            color: "var(--celerex-text-primary)",
          }}
        >
          Outline
        </span>
        <button
          onClick={onClose}
          style={{
            background: "none",
            border: "none",
            cursor: "pointer",
            fontSize: 18,
            lineHeight: 1,
            color: "var(--celerex-text-secondary)",
            padding: "2px 6px",
            borderRadius: 4,
          }}
          title="Close outline"
        >
          ×
        </button>
      </div>

      {/* Heading list */}
      <div style={{ flex: 1, overflowY: "auto", padding: "8px 0" }}>
        {headings.length === 0 ? (
          <div
            style={{
              padding: "16px",
              textAlign: "center",
              color: "var(--celerex-text-secondary)",
              fontSize: 13,
            }}
          >
            No headings found
          </div>
        ) : (
          headings.map((h, i) => (
            <div
              key={`${i}-${h.text}`}
              style={{
                padding: "6px 16px",
                paddingLeft: 16 + INDENT_MAP[h.level],
                fontSize: h.level === 1 ? 13 : 12,
                fontWeight: h.level === 1 ? 600 : h.level === 2 ? 500 : 400,
                color: "var(--celerex-text-primary)",
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
              }}
              title={h.text}
            >
              {h.text}
            </div>
          ))
        )}
      </div>
    </div>
  );
}
