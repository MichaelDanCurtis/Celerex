import { useState, useRef, useEffect, useMemo } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { engagementsAtom, openDocAtom } from "../state/atoms";

export function SearchPanel({ onClose }: { onClose: () => void }) {
  const engagements = useAtomValue(engagementsAtom);
  const openDoc = useSetAtom(openDocAtom);
  const [query, setQuery] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const results = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return [];
    return engagements
      .map((eng) => ({
        engagement: eng,
        docs: eng.docs.filter((d) =>
          (d.title || "Untitled").toLowerCase().includes(q)
        ),
      }))
      .filter((group) => group.docs.length > 0);
  }, [query, engagements]);

  const hasQuery = query.trim().length > 0;

  return (
    <div
      style={{
        position: "fixed",
        top: 0,
        left: 0,
        right: 0,
        bottom: 0,
        zIndex: 1000,
      }}
      onClick={onClose}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          position: "fixed",
          top: 56,
          left: 8,
          width: 320,
          maxHeight: 420,
          background: "var(--celerex-bg-primary)",
          border: "1px solid var(--celerex-border)",
          borderRadius: 8,
          boxShadow: "0 8px 24px rgba(0,0,0,0.18)",
          display: "flex",
          flexDirection: "column",
          overflow: "hidden",
          zIndex: 1001,
        }}
      >
        <div style={{ padding: 8, borderBottom: "1px solid var(--celerex-border)" }}>
          <input
            ref={inputRef}
            type="text"
            placeholder="Search all docs..."
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") onClose();
            }}
            style={{
              display: "block",
              width: "100%",
              height: 32,
              padding: "0 10px",
              fontSize: 13,
              border: "1px solid var(--celerex-border)",
              borderRadius: 6,
              outline: "none",
              background: "var(--celerex-bg-secondary)",
              color: "var(--celerex-text-primary)",
              boxSizing: "border-box",
            }}
          />
        </div>
        <div style={{ flex: 1, overflow: "auto", padding: "4px 0" }}>
          {hasQuery && results.length === 0 && (
            <div
              style={{
                padding: "16px 12px",
                fontSize: 13,
                color: "var(--celerex-text-secondary)",
                textAlign: "center",
              }}
            >
              No results
            </div>
          )}
          {results.map((group) => (
            <div key={group.engagement.id} style={{ marginBottom: 4 }}>
              <div
                style={{
                  fontSize: 11,
                  fontWeight: 600,
                  textTransform: "uppercase",
                  letterSpacing: "0.05em",
                  color: "var(--celerex-text-secondary)",
                  padding: "6px 12px 2px",
                }}
              >
                {group.engagement.name}
              </div>
              {group.docs.map((doc) => (
                <SearchResultItem
                  key={doc.id}
                  title={doc.title || "Untitled"}
                  engagementName={group.engagement.name}
                  onClick={() => {
                    openDoc(doc.id);
                    onClose();
                  }}
                />
              ))}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function SearchResultItem({
  title,
  engagementName,
  onClick,
}: {
  title: string;
  engagementName: string;
  onClick: () => void;
}) {
  const [hovered, setHovered] = useState(false);

  return (
    <div
      onClick={onClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        padding: "6px 12px",
        cursor: "pointer",
        background: hovered ? "var(--celerex-bg-secondary)" : "transparent",
        borderRadius: 4,
        margin: "0 4px",
        transition: "background 0.12s ease",
      }}
    >
      <div
        style={{
          fontSize: 13,
          color: "var(--celerex-text-primary)",
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
        }}
      >
        {title}
      </div>
      <div
        style={{
          fontSize: 11,
          color: "var(--celerex-text-secondary)",
          marginTop: 1,
        }}
      >
        {engagementName}
      </div>
    </div>
  );
}
