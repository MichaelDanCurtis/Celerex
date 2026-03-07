import { useState, useMemo } from "react";
import { useAtomValue } from "jotai";
import { engagementsAtom, activeDocAtom, editorStoreAtom } from "../state/atoms";

interface SplitViewProps {
  onClose: () => void;
}

function extractTextFromStore(store: any, docId: string): string {
  try {
    if (!store) return "(No editor store available)";
    const doc = store.doc?.collection?.getDoc(docId) ?? store.doc?.workspace?.getDoc(docId);
    if (!doc) return "(Document not found in store)";
    const blocks = doc.getBlocksByFlavour?.("affine:paragraph") ?? [];
    const lines: string[] = [];
    for (const block of blocks) {
      const text = block.text?.toString?.() ?? "";
      if (text) lines.push(text);
    }
    if (lines.length === 0) return "(No text content)";
    return lines.join("\n");
  } catch {
    return "(Unable to read document content)";
  }
}

export function SplitView({ onClose }: SplitViewProps) {
  const engagements = useAtomValue(engagementsAtom);
  const activeDoc = useAtomValue(activeDocAtom);
  const editorStore = useAtomValue(editorStoreAtom);

  const allDocs = useMemo(() => {
    return engagements.flatMap((eng) =>
      eng.docs.map((doc) => ({ ...doc, engagementName: eng.name }))
    );
  }, [engagements]);

  const otherDocs = useMemo(() => {
    return allDocs.filter((d) => d.id !== activeDoc?.id);
  }, [allDocs, activeDoc]);

  const [selectedDocId, setSelectedDocId] = useState<string>(
    otherDocs[0]?.id ?? ""
  );

  const selectedDoc = useMemo(() => {
    return allDocs.find((d) => d.id === selectedDocId);
  }, [allDocs, selectedDocId]);

  const previewText = useMemo(() => {
    if (!selectedDocId) return "(Select a document)";
    return extractTextFromStore(editorStore, selectedDocId);
  }, [editorStore, selectedDocId]);

  const headerStyle: React.CSSProperties = {
    height: 36,
    padding: "0 12px",
    display: "flex",
    alignItems: "center",
    gap: 8,
    borderBottom: "1px solid var(--celerex-border, #e5e7eb)",
    background: "var(--celerex-bg-secondary, #f9fafb)",
    fontSize: 13,
    fontWeight: 600,
    color: "var(--celerex-text-primary, #111)",
    flexShrink: 0,
  };

  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        borderLeft: "1px solid var(--celerex-border, #e5e7eb)",
        overflow: "hidden",
        minWidth: 0,
      }}
    >
      {/* Right panel header */}
      <div style={headerStyle}>
        <select
          value={selectedDocId}
          onChange={(e) => setSelectedDocId(e.target.value)}
          style={{
            flex: 1,
            padding: "4px 6px",
            borderRadius: 4,
            border: "1px solid var(--celerex-border, #e5e7eb)",
            background: "var(--celerex-bg-primary, #fff)",
            color: "var(--celerex-text-primary, #111)",
            fontSize: 12,
            outline: "none",
            cursor: "pointer",
          }}
        >
          {otherDocs.length === 0 && (
            <option value="">No other documents</option>
          )}
          {otherDocs.map((doc) => (
            <option key={doc.id} value={doc.id}>
              {doc.title} — {doc.engagementName}
            </option>
          ))}
        </select>
        <button
          onClick={onClose}
          style={{
            padding: "2px 8px",
            borderRadius: 4,
            border: "1px solid var(--celerex-border, #e5e7eb)",
            background: "transparent",
            color: "var(--celerex-text-secondary, #888)",
            fontSize: 14,
            cursor: "pointer",
            lineHeight: 1,
          }}
          title="Close split view"
        >
          ✕
        </button>
      </div>

      {/* Right panel content - read-only text preview */}
      <div
        style={{
          flex: 1,
          overflow: "auto",
          padding: 16,
          fontSize: 14,
          lineHeight: 1.7,
          color: "var(--celerex-text-primary, #111)",
          whiteSpace: "pre-wrap",
          fontFamily: "inherit",
        }}
      >
        {selectedDoc && (
          <div
            style={{
              marginBottom: 12,
              fontSize: 18,
              fontWeight: 700,
              color: "var(--celerex-text-primary, #111)",
            }}
          >
            {selectedDoc.title}
          </div>
        )}
        {previewText}
      </div>
    </div>
  );
}
