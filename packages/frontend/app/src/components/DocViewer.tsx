import { useEffect, useState } from "react";
import { getDoc, listDocBlocks, type ApiDoc, type ApiDocBlock } from "../api/client";

export function DocViewer({ token, docId }: { token: string; docId: string }) {
  const [doc, setDoc] = useState<ApiDoc | null>(null);
  const [blocks, setBlocks] = useState<ApiDocBlock[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!token || !docId) return;
    let cancelled = false;

    async function load() {
      setLoading(true);
      setError(null);
      try {
        const [d, b] = await Promise.all([
          getDoc(token, docId),
          listDocBlocks(token, docId),
        ]);
        if (cancelled) return;
        setDoc(d);
        setBlocks(b);
      } catch (e) {
        if (cancelled) return;
        setError(e instanceof Error ? e.message : "Failed to load document");
      } finally {
        if (!cancelled) setLoading(false);
      }
    }

    void load();
    return () => {
      cancelled = true;
    };
  }, [token, docId]);

  if (loading) {
    return (
      <div
        data-testid="doc-viewer-loading"
        style={{ padding: 16, color: "var(--celerex-text-secondary)" }}
      >
        Loading document…
      </div>
    );
  }

  if (error) {
    return (
      <div
        data-testid="doc-viewer-error"
        style={{
          padding: 16,
          color: "#dc2626",
          background: "rgba(220, 38, 38, 0.06)",
          borderRadius: 6,
          margin: 16,
        }}
      >
        Error loading document: {error}
      </div>
    );
  }

  if (!doc) {
    return (
      <div style={{ padding: 16, color: "var(--celerex-text-secondary)" }}>
        Document not found.
      </div>
    );
  }

  return (
    <div
      data-testid="doc-viewer"
      style={{
        padding: 16,
        flex: 1,
        overflow: "auto",
        background: "var(--celerex-bg-primary)",
      }}
    >
      <header
        style={{
          marginBottom: 16,
          paddingBottom: 12,
          borderBottom: "1px solid var(--celerex-border)",
        }}
      >
        <h2 data-testid="doc-viewer-title" style={{ margin: "0 0 4px 0" }}>
          {doc.title || "Untitled"}
        </h2>
        <div style={{ fontSize: 12, color: "var(--celerex-text-secondary)" }}>
          Created: {formatDate(doc.created_at)}
        </div>
      </header>

      <div data-testid="doc-viewer-blocks">
        {blocks.length === 0 ? (
          <div
            style={{
              padding: 24,
              textAlign: "center",
              color: "var(--celerex-text-secondary)",
              fontStyle: "italic",
            }}
          >
            This document has no blocks yet.
          </div>
        ) : (
          blocks.map((block) => (
            <BlockRenderer key={block.id} block={block} />
          ))
        )}
      </div>
    </div>
  );
}

function BlockRenderer({ block }: { block: ApiDocBlock }) {
  const payload = block.payload as { type?: string; text?: string } | null;
  const type = payload && typeof payload === "object" ? payload.type : undefined;

  return (
    <div
      data-testid={`doc-block-${block.id}`}
      data-block-type={type ?? "unknown"}
      style={{
        marginBottom: 12,
        padding: 12,
        border: "1px solid var(--celerex-border)",
        borderRadius: 6,
        background: "var(--celerex-bg-secondary, transparent)",
      }}
    >
      {type === "markdown" ? (
        <div
          className="whitespace-pre-wrap"
          style={{
            whiteSpace: "pre-wrap",
            fontSize: 14,
            lineHeight: 1.5,
            fontFamily: "inherit",
            margin: 0,
          }}
        >
          {typeof payload?.text === "string"
            ? payload.text
            : JSON.stringify(payload)}
        </div>
      ) : type === "json" ? (
        <pre
          style={{
            margin: 0,
            fontSize: 12,
            fontFamily:
              "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace",
            whiteSpace: "pre-wrap",
            wordBreak: "break-word",
          }}
        >
          {JSON.stringify(block.payload, null, 2)}
        </pre>
      ) : (
        <pre
          style={{
            margin: 0,
            fontSize: 12,
            fontFamily:
              "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace",
            whiteSpace: "pre-wrap",
            wordBreak: "break-word",
            color: "var(--celerex-text-secondary)",
          }}
        >
          {JSON.stringify(block.payload)}
        </pre>
      )}
    </div>
  );
}

function formatDate(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleString();
  } catch {
    return iso;
  }
}
