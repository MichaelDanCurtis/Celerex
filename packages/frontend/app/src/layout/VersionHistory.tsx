import { useState } from "react";

interface VersionHistoryProps {
  onClose: () => void;
}

interface Revision {
  id: string;
  timestamp: number;
  summary: string;
}

const MOCK_REVISIONS: Revision[] = [
  { id: "rev-6", timestamp: Date.now() - 1000 * 60 * 8, summary: "Added 3 paragraphs to introduction" },
  { id: "rev-5", timestamp: Date.now() - 1000 * 60 * 47, summary: "Deleted heading \"Old Summary\"" },
  { id: "rev-4", timestamp: Date.now() - 1000 * 60 * 60 * 3, summary: "Reformatted bullet list into table" },
  { id: "rev-3", timestamp: Date.now() - 1000 * 60 * 60 * 26, summary: "Inserted image and caption" },
  { id: "rev-2", timestamp: Date.now() - 1000 * 60 * 60 * 72, summary: "Edited conclusion paragraph" },
  { id: "rev-1", timestamp: Date.now() - 1000 * 60 * 60 * 168, summary: "Initial document creation" },
];

function formatTimestamp(ts: number): string {
  const diff = Date.now() - ts;
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return "Just now";
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days}d ago`;
  return new Date(ts).toLocaleDateString();
}

export function VersionHistory({ onClose }: VersionHistoryProps) {
  const [selectedId, setSelectedId] = useState<string | null>(null);

  return (
    <div
      style={{
        width: 300,
        minWidth: 300,
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
          Version History
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
          title="Close version history"
        >
          ×
        </button>
      </div>

      {/* Revision list */}
      <div style={{ flex: 1, overflowY: "auto", padding: "8px 0" }}>
        {MOCK_REVISIONS.map((rev) => {
          const isSelected = selectedId === rev.id;
          return (
            <div
              key={rev.id}
              onClick={() => setSelectedId(rev.id)}
              style={{
                padding: "10px 16px",
                cursor: "pointer",
                borderLeft: isSelected
                  ? "3px solid var(--celerex-accent)"
                  : "3px solid transparent",
                background: isSelected
                  ? "var(--celerex-bg-secondary)"
                  : "transparent",
                transition: "background 0.15s ease",
              }}
            >
              <div
                style={{
                  fontSize: 13,
                  fontWeight: 500,
                  color: "var(--celerex-text-primary)",
                  marginBottom: 4,
                }}
              >
                {rev.summary}
              </div>
              <div
                style={{
                  fontSize: 11,
                  color: "var(--celerex-text-secondary)",
                }}
              >
                {formatTimestamp(rev.timestamp)}
              </div>

              {isSelected && (
                <button
                  disabled
                  style={{
                    marginTop: 8,
                    padding: "4px 12px",
                    fontSize: 12,
                    fontWeight: 500,
                    borderRadius: 4,
                    border: "1px solid var(--celerex-border)",
                    background: "var(--celerex-bg-secondary)",
                    color: "var(--celerex-text-secondary)",
                    cursor: "not-allowed",
                    opacity: 0.6,
                  }}
                  title="Restore functionality coming soon"
                >
                  Restore
                </button>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
