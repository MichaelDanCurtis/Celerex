import { useMemo } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { engagementsAtom, openDocAtom, activeViewAtom, type DocMeta } from "../state/atoms";
import { docTagsAtom, tagsAtom } from "../state/tagsAtom";

interface TimelineDoc extends DocMeta {
  engagementName: string;
  tags: { id: string; name: string; color: string }[];
}

interface TimelineGroup {
  label: string;
  docs: TimelineDoc[];
}

function classifyDate(timestamp: number): string {
  const now = new Date();
  const date = new Date(timestamp);

  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const startOfYesterday = startOfToday - 86400000;
  const dayOfWeek = now.getDay();
  const startOfWeek = startOfToday - dayOfWeek * 86400000;

  if (timestamp >= startOfToday) return "Today";
  if (timestamp >= startOfYesterday) return "Yesterday";
  if (timestamp >= startOfWeek) return "This Week";
  return "Earlier";
}

function formatTime(timestamp: number): string {
  const d = new Date(timestamp);
  return d.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function TimelineView() {
  const engagements = useAtomValue(engagementsAtom);
  const docTags = useAtomValue(docTagsAtom);
  const allTags = useAtomValue(tagsAtom);
  const openDoc = useSetAtom(openDocAtom);
  const setActiveView = useSetAtom(activeViewAtom);

  const groups = useMemo<TimelineGroup[]>(() => {
    const tagMap = new Map(allTags.map((t) => [t.id, t]));
    const allDocs: TimelineDoc[] = engagements.flatMap((eng) =>
      eng.docs.map((doc) => ({
        ...doc,
        engagementName: eng.name,
        tags: (docTags[doc.id] ?? [])
          .map((tid) => tagMap.get(tid))
          .filter(Boolean) as TimelineDoc["tags"],
      }))
    );

    // Sort newest first
    allDocs.sort((a, b) => b.createdAt - a.createdAt);

    const order = ["Today", "Yesterday", "This Week", "Earlier"];
    const buckets = new Map<string, TimelineDoc[]>();
    for (const label of order) buckets.set(label, []);

    for (const doc of allDocs) {
      const label = classifyDate(doc.createdAt);
      buckets.get(label)!.push(doc);
    }

    return order
      .map((label) => ({ label, docs: buckets.get(label)! }))
      .filter((g) => g.docs.length > 0);
  }, [engagements, docTags, allTags]);

  const handleDocClick = (docId: string) => {
    openDoc(docId);
    setActiveView("editor");
  };

  return (
    <div
      style={{
        flex: 1,
        overflow: "auto",
        padding: "32px 48px",
        background: "var(--celerex-bg-primary)",
        color: "var(--celerex-text-primary)",
      }}
    >
      <h2
        style={{
          fontSize: 22,
          fontWeight: 700,
          marginBottom: 24,
          letterSpacing: "-0.02em",
        }}
      >
        Timeline
      </h2>

      {groups.length === 0 && (
        <p style={{ color: "var(--celerex-text-secondary)", fontSize: 14 }}>
          No documents yet.
        </p>
      )}

      {groups.map((group) => (
        <div key={group.label} style={{ marginBottom: 32 }}>
          {/* Group header */}
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 8,
              marginBottom: 12,
              paddingLeft: 28,
            }}
          >
            <span
              style={{
                fontSize: 14,
                fontWeight: 600,
                color: "var(--celerex-text-primary)",
              }}
            >
              {group.label}
            </span>
            <span
              style={{
                fontSize: 11,
                fontWeight: 500,
                color: "var(--celerex-text-secondary)",
                background: "var(--celerex-bg-secondary, rgba(0,0,0,0.05))",
                borderRadius: 10,
                padding: "1px 8px",
              }}
            >
              {group.docs.length}
            </span>
          </div>

          {/* Timeline entries */}
          <div style={{ position: "relative", paddingLeft: 28 }}>
            {/* Vertical timeline line */}
            <div
              style={{
                position: "absolute",
                left: 9,
                top: 0,
                bottom: 0,
                width: 2,
                background: "var(--celerex-border)",
                borderRadius: 1,
              }}
            />

            {group.docs.map((doc) => (
              <div
                key={doc.id}
                onClick={() => handleDocClick(doc.id)}
                style={{
                  position: "relative",
                  padding: "10px 14px",
                  marginBottom: 4,
                  borderRadius: 8,
                  cursor: "pointer",
                  transition: "background 0.15s ease",
                  background: "transparent",
                }}
                onMouseEnter={(e) => {
                  (e.currentTarget as HTMLElement).style.background =
                    "var(--celerex-bg-secondary, rgba(0,0,0,0.03))";
                }}
                onMouseLeave={(e) => {
                  (e.currentTarget as HTMLElement).style.background = "transparent";
                }}
              >
                {/* Timeline dot */}
                <div
                  style={{
                    position: "absolute",
                    left: -23,
                    top: 16,
                    width: 10,
                    height: 10,
                    borderRadius: "50%",
                    background: "var(--celerex-accent, #3b82f6)",
                    border: "2px solid var(--celerex-bg-primary)",
                    boxSizing: "border-box",
                  }}
                />

                {/* Doc title */}
                <div
                  style={{
                    fontSize: 14,
                    fontWeight: 500,
                    color: "var(--celerex-text-primary)",
                    marginBottom: 4,
                  }}
                >
                  {doc.title || "Untitled"}
                </div>

                {/* Meta row */}
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 10,
                    flexWrap: "wrap",
                  }}
                >
                  <span
                    style={{
                      fontSize: 12,
                      color: "var(--celerex-text-secondary)",
                    }}
                  >
                    {doc.engagementName}
                  </span>
                  <span
                    style={{
                      fontSize: 11,
                      color: "var(--celerex-text-secondary)",
                      opacity: 0.7,
                    }}
                  >
                    {formatTime(doc.createdAt)}
                  </span>

                  {/* Tags */}
                  {doc.tags.length > 0 && (
                    <div style={{ display: "flex", gap: 4 }}>
                      {doc.tags.map((tag) => (
                        <span
                          key={tag.id}
                          style={{
                            fontSize: 10,
                            fontWeight: 500,
                            padding: "1px 6px",
                            borderRadius: 4,
                            background: tag.color + "22",
                            color: tag.color,
                            whiteSpace: "nowrap",
                          }}
                        >
                          {tag.name}
                        </span>
                      ))}
                    </div>
                  )}
                </div>
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
