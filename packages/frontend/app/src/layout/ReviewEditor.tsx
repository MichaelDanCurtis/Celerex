import { useEffect, useMemo, useRef, useState } from "react";
import { useAtomValue } from "jotai";
import { editorStoreAtom } from "../state/atoms";
import { getWorkspace } from "../editor/setup";
import { BlockSuiteEditor } from "../editor/BlockSuiteEditor";
import { insertContentIntoDoc } from "../editor/insertContent";
import { extractMarkdown } from "../editor/extractMarkdown";

interface ReviewEditorProps {
  draft: string;
  flowName: string | null;
  prompt: string;
  busy: boolean;
  /** Called with the editor's current content (markdown) on approve. */
  onApprove: (content: string) => void;
  onReject: () => void;
  onClose: () => void;
}

/**
 * Full-screen rich review surface: opens the agent's draft in the REAL
 * BlockSuite editor so a reviewer can edit it with full formatting before
 * approving. Seeds a transient doc from the draft markdown, and on approve
 * serializes the (edited) blocks back to markdown — which the review API
 * accepts as the content that flows downstream. No backend change required.
 */
export function ReviewEditor({
  draft,
  flowName,
  prompt,
  busy,
  onApprove,
  onReject,
  onClose,
}: ReviewEditorProps) {
  const workspace = useMemo(() => getWorkspace(), []);
  // A fresh transient doc per review session — never persisted server-side.
  const docId = useMemo(
    () => `review-${Date.now().toString(36)}-${Math.floor(Math.random() * 1e6).toString(36)}`,
    [],
  );
  const store = useAtomValue(editorStoreAtom);
  const seeded = useRef(false);

  // Seed the draft once the inner editor's store is live.
  useEffect(() => {
    if (!store || seeded.current) return;
    seeded.current = true;
    try {
      // Remove the editor's placeholder empty paragraph(s) before inserting.
      const note = store.root?.children?.find(
        (c: any) => c.flavour === "affine:note",
      );
      if (note) {
        for (const child of [...(note.children ?? [])]) {
          const empty = !(child?.text?.toString?.() ?? "").trim();
          if (child.flavour === "affine:paragraph" && empty) {
            try {
              store.deleteBlock(child);
            } catch {
              /* best-effort cleanup */
            }
          }
        }
      }
      insertContentIntoDoc(store, draft);
    } catch (e) {
      // If seeding fails the editor still opens (empty) — the reviewer can
      // fall back to the inline pane. Don't crash the surface.
      console.error("ReviewEditor: failed to seed draft", e);
    }
  }, [store, draft]);

  const [escHint, setEscHint] = useState(false);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (busy) return;
        setEscHint(true);
        setTimeout(() => setEscHint(false), 1500);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [busy]);

  function handleApprove() {
    const md = store ? extractMarkdown(store) : draft;
    onApprove(md.trim() ? md : draft);
  }

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 1000,
        display: "flex",
        flexDirection: "column",
        background: "var(--celerex-bg-primary)",
      }}
    >
      {/* Header */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 12,
          padding: "12px 18px",
          borderBottom: "1px solid var(--celerex-border)",
          background: "var(--celerex-bg-secondary, #13161f)",
        }}
      >
        <button
          onClick={onClose}
          disabled={busy}
          title="Back to the approvals inbox"
          style={{
            padding: "6px 12px",
            borderRadius: 7,
            border: "1px solid var(--celerex-border)",
            background: "transparent",
            color: "var(--celerex-text-secondary)",
            cursor: busy ? "not-allowed" : "pointer",
            fontSize: 13,
            fontWeight: 600,
          }}
        >
          ← Inbox
        </button>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ fontSize: 14, fontWeight: 700, color: "var(--celerex-text-primary)" }}>
            {flowName ?? "Flow"} — review &amp; approve
          </div>
          <div style={{ fontSize: 12.5, color: "var(--celerex-text-secondary)" }}>{prompt}</div>
        </div>
        {escHint && (
          <span style={{ fontSize: 12, color: "var(--celerex-text-secondary)" }}>
            Use the buttons → to finish
          </span>
        )}
        <button
          data-testid="editor-reject"
          onClick={onReject}
          disabled={busy}
          style={{
            padding: "8px 16px",
            borderRadius: 8,
            border: "1px solid var(--celerex-border)",
            background: "transparent",
            color: "var(--celerex-text-secondary)",
            fontSize: 13,
            fontWeight: 600,
            cursor: busy ? "not-allowed" : "pointer",
          }}
        >
          Request changes
        </button>
        <button
          data-testid="editor-approve"
          onClick={handleApprove}
          disabled={busy}
          style={{
            padding: "8px 18px",
            borderRadius: 8,
            border: "none",
            background: "#16a34a",
            color: "#fff",
            fontSize: 13,
            fontWeight: 700,
            opacity: busy ? 0.6 : 1,
            cursor: busy ? "not-allowed" : "pointer",
          }}
        >
          {busy ? "Working…" : "Approve & continue"}
        </button>
      </div>

      {/* The real BlockSuite editor */}
      <div style={{ flex: 1, minHeight: 0, overflow: "hidden" }}>
        <BlockSuiteEditor mode="page" workspace={workspace} docId={docId} />
      </div>
    </div>
  );
}
