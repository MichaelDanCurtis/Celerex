import { useState, useEffect, useRef, useMemo } from "react";
import { type DocTemplate, templates } from "../templates/templates";

interface TemplateDialogProps {
  open: boolean;
  onClose: () => void;
  onSelect: (template: DocTemplate) => void;
}

const categoryLabels: Record<DocTemplate["category"], string> = {
  general: "General",
  business: "Business",
  research: "Research",
};

const categoryOrder: DocTemplate["category"][] = ["general", "business", "research"];

const badgeColors: Record<DocTemplate["category"], { bg: string; text: string }> = {
  general: { bg: "rgba(59,130,246,0.12)", text: "#3b82f6" },
  business: { bg: "rgba(168,85,247,0.12)", text: "#a855f7" },
  research: { bg: "rgba(34,197,94,0.12)", text: "#22c55e" },
};

export function TemplateDialog({ open, onClose, onSelect }: TemplateDialogProps) {
  const [query, setQuery] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) {
      setQuery("");
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [open]);

  const filtered = useMemo(() => {
    if (!query.trim()) return templates;
    const q = query.toLowerCase();
    return templates.filter(
      (t) =>
        t.name.toLowerCase().includes(q) ||
        t.description.toLowerCase().includes(q) ||
        t.category.toLowerCase().includes(q)
    );
  }, [query]);

  const grouped = useMemo(() => {
    const map = new Map<DocTemplate["category"], DocTemplate[]>();
    for (const t of filtered) {
      const list = map.get(t.category) ?? [];
      list.push(t);
      map.set(t.category, list);
    }
    return categoryOrder.filter((c) => map.has(c)).map((c) => ({ category: c, items: map.get(c)! }));
  }, [filtered]);

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === "Escape") {
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
        paddingTop: 80,
        background: "rgba(0, 0, 0, 0.4)",
      }}
      onClick={onClose}
    >
      <div
        style={{
          width: "100%",
          maxWidth: 640,
          background: "var(--celerex-bg-primary, #fff)",
          borderRadius: 12,
          boxShadow: "0 16px 48px rgba(0,0,0,0.2)",
          overflow: "hidden",
          display: "flex",
          flexDirection: "column",
          maxHeight: "min(560px, 70vh)",
        }}
        onClick={(e) => e.stopPropagation()}
        onKeyDown={handleKeyDown}
      >
        {/* Header */}
        <div
          style={{
            padding: "16px 20px 12px",
            borderBottom: "1px solid var(--celerex-border, #e5e7eb)",
          }}
        >
          <div
            style={{
              fontSize: 16,
              fontWeight: 600,
              color: "var(--celerex-text-primary, #111)",
              marginBottom: 10,
            }}
          >
            New from Template
          </div>
          <input
            ref={inputRef}
            type="text"
            placeholder="Search templates..."
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            style={{
              width: "100%",
              padding: "8px 12px",
              border: "1px solid var(--celerex-border, #e5e7eb)",
              borderRadius: 8,
              outline: "none",
              fontSize: 14,
              background: "var(--celerex-bg-secondary, #f9fafb)",
              color: "var(--celerex-text-primary, #111)",
            }}
          />
        </div>

        {/* Template grid */}
        <div style={{ overflowY: "auto", padding: "12px 20px 16px" }}>
          {grouped.length === 0 && (
            <div
              style={{
                padding: 24,
                textAlign: "center",
                color: "var(--celerex-text-secondary, #888)",
                fontSize: 13,
              }}
            >
              No templates match your search
            </div>
          )}
          {grouped.map(({ category, items }) => (
            <div key={category} style={{ marginBottom: 16 }}>
              <div
                style={{
                  fontSize: 11,
                  fontWeight: 600,
                  textTransform: "uppercase",
                  letterSpacing: "0.05em",
                  color: "var(--celerex-text-secondary, #888)",
                  marginBottom: 8,
                }}
              >
                {categoryLabels[category]}
              </div>
              <div
                style={{
                  display: "grid",
                  gridTemplateColumns: "repeat(auto-fill, minmax(180px, 1fr))",
                  gap: 10,
                }}
              >
                {items.map((template) => (
                  <div
                    key={template.id}
                    onClick={() => {
                      onSelect(template);
                      onClose();
                    }}
                    style={{
                      padding: "12px 14px",
                      borderRadius: 8,
                      border: "1px solid var(--celerex-border, #e5e7eb)",
                      cursor: "pointer",
                      transition: "border-color 0.15s, box-shadow 0.15s",
                      background: "var(--celerex-bg-primary, #fff)",
                    }}
                    onMouseEnter={(e) => {
                      e.currentTarget.style.borderColor = "var(--celerex-accent, #3b82f6)";
                      e.currentTarget.style.boxShadow = "0 0 0 1px var(--celerex-accent, #3b82f6)";
                    }}
                    onMouseLeave={(e) => {
                      e.currentTarget.style.borderColor = "var(--celerex-border, #e5e7eb)";
                      e.currentTarget.style.boxShadow = "none";
                    }}
                  >
                    <div
                      style={{
                        fontSize: 13,
                        fontWeight: 600,
                        color: "var(--celerex-text-primary, #111)",
                        marginBottom: 4,
                      }}
                    >
                      {template.name}
                    </div>
                    <div
                      style={{
                        fontSize: 12,
                        color: "var(--celerex-text-secondary, #888)",
                        lineHeight: 1.4,
                        marginBottom: 8,
                      }}
                    >
                      {template.description}
                    </div>
                    <span
                      style={{
                        display: "inline-block",
                        fontSize: 10,
                        fontWeight: 600,
                        textTransform: "uppercase",
                        letterSpacing: "0.04em",
                        padding: "2px 6px",
                        borderRadius: 4,
                        background: badgeColors[template.category].bg,
                        color: badgeColors[template.category].text,
                      }}
                    >
                      {categoryLabels[template.category]}
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
            padding: "8px 20px",
            borderTop: "1px solid var(--celerex-border, #e5e7eb)",
            fontSize: 11,
            color: "var(--celerex-text-secondary, #888)",
          }}
        >
          Click a template to create a new document. Press esc to close.
        </div>
      </div>
    </div>
  );
}
