import { useAtomValue, useSetAtom } from "jotai";
import { trashAtom, restoreFromTrashAtom, permanentDeleteAtom } from "../state/trashAtom";

interface TrashBinProps {
  onClose: () => void;
}

function formatDate(ts: number): string {
  const d = new Date(ts);
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
}

export function TrashBin({ onClose }: TrashBinProps) {
  const trash = useAtomValue(trashAtom);
  const restore = useSetAtom(restoreFromTrashAtom);
  const deletePermanently = useSetAtom(permanentDeleteAtom);

  return (
    <div
      style={{
        width: 320,
        borderLeft: "1px solid var(--celerex-border, #e5e7eb)",
        background: "var(--celerex-bg-primary, #fff)",
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
      }}
    >
      {/* Header */}
      <div
        style={{
          padding: "12px 16px",
          borderBottom: "1px solid var(--celerex-border, #e5e7eb)",
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
        }}
      >
        <span style={{ fontWeight: 600, fontSize: 14, color: "var(--celerex-text-primary, #111)" }}>
          Trash
        </span>
        <button
          onClick={onClose}
          style={{
            background: "none",
            border: "none",
            cursor: "pointer",
            fontSize: 16,
            color: "var(--celerex-text-secondary, #888)",
            lineHeight: 1,
            padding: "2px 6px",
            borderRadius: 4,
          }}
          title="Close"
        >
          ✕
        </button>
      </div>

      {/* List */}
      <div style={{ flex: 1, overflowY: "auto", padding: "8px 0" }}>
        {trash.length === 0 ? (
          <div
            style={{
              padding: 24,
              textAlign: "center",
              color: "var(--celerex-text-secondary, #888)",
              fontSize: 13,
            }}
          >
            Trash is empty
          </div>
        ) : (
          trash.map((item) => (
            <div
              key={item.id}
              style={{
                padding: "10px 16px",
                borderBottom: "1px solid var(--celerex-border, #e5e7eb)",
                display: "flex",
                flexDirection: "column",
                gap: 6,
              }}
            >
              <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
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
                  {item.title}
                </span>
                <span style={{ fontSize: 11, color: "var(--celerex-text-secondary, #888)" }}>
                  {item.engagementName} &middot; Deleted {formatDate(item.deletedAt)}
                </span>
              </div>
              <div style={{ display: "flex", gap: 6 }}>
                <button
                  onClick={() => restore(item.id)}
                  style={{
                    padding: "4px 10px",
                    borderRadius: 5,
                    border: "1px solid var(--celerex-border, #e5e7eb)",
                    background: "transparent",
                    color: "var(--celerex-accent, #3b82f6)",
                    fontSize: 12,
                    cursor: "pointer",
                    fontWeight: 500,
                  }}
                >
                  Restore
                </button>
                <button
                  onClick={() => deletePermanently(item.id)}
                  style={{
                    padding: "4px 10px",
                    borderRadius: 5,
                    border: "1px solid var(--celerex-border, #e5e7eb)",
                    background: "transparent",
                    color: "#ef4444",
                    fontSize: 12,
                    cursor: "pointer",
                    fontWeight: 500,
                  }}
                >
                  Delete Forever
                </button>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
