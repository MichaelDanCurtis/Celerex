import { useAtomValue } from "jotai";
import { activityLogAtom } from "../state/activityAtom";

interface ActivityLogProps {
  onClose: () => void;
}

const USER_COLORS: Record<string, string> = {
  Alice: "#6366f1",
  Bob: "#f59e0b",
  Carol: "#10b981",
};

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

export function ActivityLog({ onClose }: ActivityLogProps) {
  const entries = useAtomValue(activityLogAtom);

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
          Activity
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
          title="Close activity log"
        >
          ×
        </button>
      </div>

      {/* Entry list */}
      <div style={{ flex: 1, overflowY: "auto", padding: "8px 0" }}>
        {entries.map((entry) => {
          const avatarColor = USER_COLORS[entry.user] || "#9ca3af";
          const initial = entry.user.charAt(0).toUpperCase();

          return (
            <div
              key={entry.id}
              style={{
                padding: "10px 16px",
                display: "flex",
                alignItems: "flex-start",
                gap: 10,
              }}
            >
              {/* User avatar */}
              <div
                style={{
                  width: 28,
                  height: 28,
                  borderRadius: "50%",
                  background: avatarColor,
                  color: "#fff",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  fontSize: 12,
                  fontWeight: 600,
                  flexShrink: 0,
                }}
              >
                {initial}
              </div>

              {/* Content */}
              <div style={{ flex: 1, minWidth: 0 }}>
                <div
                  style={{
                    fontSize: 13,
                    fontWeight: 500,
                    color: "var(--celerex-text-primary)",
                    lineHeight: 1.4,
                  }}
                >
                  {entry.action}{" "}
                  <span style={{ fontWeight: 600 }}>'{entry.target}'</span>
                </div>
                <div
                  style={{
                    fontSize: 11,
                    color: "var(--celerex-text-secondary)",
                    marginTop: 2,
                  }}
                >
                  {entry.user} · {formatTimestamp(entry.timestamp)}
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
