import { useEffect, useState, useMemo } from "react";
import { useAtom, useAtomValue } from "jotai";
import {
  tokenAtom,
  activeEngagementIdAtom,
  activeViewerDocIdAtom,
  engagementsAtom,
  engagementsForActiveProjectAtom,
} from "../state/atoms";
import { listEngagementDocs, type ApiDocWithCount } from "../api/client";
import { DocViewer } from "../components/DocViewer";

export function DocsPage() {
  const token = useAtomValue(tokenAtom);
  const allEngagements = useAtomValue(engagementsAtom);
  const visibleEngagements = useAtomValue(engagementsForActiveProjectAtom);
  const [activeEngagementId, setActiveEngagementId] = useAtom(
    activeEngagementIdAtom,
  );
  const [activeViewerDocId, setActiveViewerDocId] = useAtom(
    activeViewerDocIdAtom,
  );

  // Pick a default engagement when none is selected:
  // prefer one from the active project, then any engagement.
  const effectiveEngagementId = useMemo(() => {
    if (activeEngagementId) return activeEngagementId;
    if (visibleEngagements.length > 0) return visibleEngagements[0].id;
    if (allEngagements.length > 0) return allEngagements[0].id;
    return null;
  }, [activeEngagementId, visibleEngagements, allEngagements]);

  const [docs, setDocs] = useState<ApiDocWithCount[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!token || !effectiveEngagementId) {
      setDocs([]);
      return;
    }
    let cancelled = false;

    async function load() {
      setLoading(true);
      setError(null);
      try {
        const result = await listEngagementDocs(token, effectiveEngagementId!);
        if (cancelled) return;
        setDocs(result);
      } catch (e) {
        if (cancelled) return;
        setError(e instanceof Error ? e.message : "Failed to load docs");
      } finally {
        if (!cancelled) setLoading(false);
      }
    }

    void load();
    return () => {
      cancelled = true;
    };
  }, [token, effectiveEngagementId]);

  // Gate: missing token
  if (!token) {
    return (
      <div style={{ padding: 16, flex: 1 }}>
        <Banner kind="warn">
          You are not signed in. Set a JWT in Settings to view documents.
        </Banner>
      </div>
    );
  }

  // Gate: no engagement available
  if (!effectiveEngagementId) {
    return (
      <div style={{ padding: 16, flex: 1 }}>
        <Banner kind="warn">
          No engagement selected. Create a workspace from the sidebar to get
          started.
        </Banner>
      </div>
    );
  }

  return (
    <div
      data-testid="docs-page"
      style={{ flex: 1, display: "flex", overflow: "hidden" }}
    >
      {/* Left: doc list */}
      <div
        style={{
          width: 320,
          borderRight: "1px solid var(--celerex-border)",
          overflow: "auto",
          padding: 16,
        }}
      >
        <div
          style={{
            display: "flex",
            alignItems: "baseline",
            justifyContent: "space-between",
            marginBottom: 8,
          }}
        >
          <h2 style={{ margin: 0, fontSize: 16 }}>Documents</h2>
          {loading && (
            <span
              style={{ fontSize: 11, color: "var(--celerex-text-secondary)" }}
            >
              Loading…
            </span>
          )}
        </div>

        {/* Engagement switcher (if more than one is visible) */}
        {visibleEngagements.length > 1 && (
          <select
            data-testid="docs-engagement-select"
            value={effectiveEngagementId}
            onChange={(e) => {
              setActiveEngagementId(e.target.value);
              setActiveViewerDocId(null);
            }}
            style={{
              width: "100%",
              padding: "6px 8px",
              fontSize: 12,
              marginBottom: 12,
              borderRadius: 6,
              border: "1px solid var(--celerex-border)",
              background: "var(--celerex-bg-primary)",
              color: "var(--celerex-text-primary)",
            }}
          >
            {visibleEngagements.map((eng) => (
              <option key={eng.id} value={eng.id}>
                {eng.name}
              </option>
            ))}
          </select>
        )}

        {error && (
          <Banner kind="error">
            Failed to load: {error}
          </Banner>
        )}

        {!loading && !error && docs.length === 0 && (
          <div
            data-testid="docs-empty"
            style={{
              padding: 16,
              color: "var(--celerex-text-secondary)",
              fontStyle: "italic",
              fontSize: 13,
              textAlign: "center",
            }}
          >
            No docs yet.
          </div>
        )}

        {docs.map((d) => (
          <div
            key={d.id}
            data-testid={`docs-row-${d.id}`}
            onClick={() => setActiveViewerDocId(d.id)}
            style={{
              padding: "8px 10px",
              borderRadius: 6,
              cursor: "pointer",
              marginBottom: 4,
              background:
                activeViewerDocId === d.id
                  ? "rgba(59, 130, 246, 0.10)"
                  : "transparent",
              color:
                activeViewerDocId === d.id
                  ? "var(--celerex-accent)"
                  : "var(--celerex-text-primary)",
              border:
                activeViewerDocId === d.id
                  ? "1px solid rgba(59, 130, 246, 0.3)"
                  : "1px solid transparent",
            }}
          >
            <div
              style={{
                fontSize: 13,
                fontWeight: 500,
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
              }}
            >
              {d.title || "Untitled"}
            </div>
            <div
              style={{
                fontSize: 11,
                color: "var(--celerex-text-secondary)",
                marginTop: 2,
                display: "flex",
                justifyContent: "space-between",
                gap: 8,
              }}
            >
              <span>
                {d.block_count} block{d.block_count === 1 ? "" : "s"}
              </span>
              <span>{formatRelative(d.created_at)}</span>
            </div>
          </div>
        ))}
      </div>

      {/* Right: viewer */}
      <div style={{ flex: 1, overflow: "hidden", display: "flex" }}>
        {activeViewerDocId ? (
          <DocViewer token={token} docId={activeViewerDocId} />
        ) : (
          <div
            style={{
              flex: 1,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              color: "var(--celerex-text-secondary)",
              fontStyle: "italic",
            }}
          >
            Select a document to view.
          </div>
        )}
      </div>
    </div>
  );
}

function Banner({
  kind,
  children,
}: {
  kind: "warn" | "error";
  children: React.ReactNode;
}) {
  const palette =
    kind === "error"
      ? { bg: "rgba(220, 38, 38, 0.08)", fg: "#dc2626", border: "#dc2626" }
      : { bg: "rgba(217, 119, 6, 0.08)", fg: "#b45309", border: "#d97706" };

  return (
    <div
      style={{
        padding: "10px 12px",
        background: palette.bg,
        color: palette.fg,
        border: `1px solid ${palette.border}`,
        borderRadius: 6,
        fontSize: 13,
        marginBottom: 12,
      }}
    >
      {children}
    </div>
  );
}

function formatRelative(iso: string): string {
  try {
    const then = new Date(iso).getTime();
    const now = Date.now();
    const diffSec = Math.max(0, Math.round((now - then) / 1000));
    if (diffSec < 60) return `${diffSec}s ago`;
    const diffMin = Math.round(diffSec / 60);
    if (diffMin < 60) return `${diffMin}m ago`;
    const diffHr = Math.round(diffMin / 60);
    if (diffHr < 24) return `${diffHr}h ago`;
    const diffDay = Math.round(diffHr / 24);
    if (diffDay < 30) return `${diffDay}d ago`;
    return new Date(iso).toLocaleDateString();
  } catch {
    return iso;
  }
}
