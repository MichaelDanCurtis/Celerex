import { useState, useEffect, useRef, useCallback } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { editorStoreAtom } from "../state/atoms";
import { addToastAtom } from "../state/toastAtom";

interface FindReplaceProps {
  open: boolean;
  onClose: () => void;
}

function extractTextFromStore(store: any): string[] {
  try {
    const root = store?.doc?.getBlockByFlavour?.("affine:page")?.[0];
    if (!root) return [];
    const blocks: string[] = [];
    function walk(block: any) {
      const text = block?.text?.toString?.();
      if (text) blocks.push(text);
      const children = block?.children ?? [];
      for (const child of children) walk(child);
    }
    // Try walking from root children
    const note = store?.doc?.getBlockByFlavour?.("affine:note")?.[0];
    if (note) walk(note);
    else if (root) walk(root);
    return blocks;
  } catch {
    return [];
  }
}

export function FindReplace({ open, onClose }: FindReplaceProps) {
  const [findQuery, setFindQuery] = useState("");
  const [replaceQuery, setReplaceQuery] = useState("");
  const [matchCount, setMatchCount] = useState(0);
  const [currentMatch, setCurrentMatch] = useState(0);
  const findInputRef = useRef<HTMLInputElement>(null);
  const editorStore = useAtomValue(editorStoreAtom);
  const addToast = useSetAtom(addToastAtom);

  // Focus input when opened
  useEffect(() => {
    if (open) {
      setFindQuery("");
      setReplaceQuery("");
      setMatchCount(0);
      setCurrentMatch(0);
      requestAnimationFrame(() => findInputRef.current?.focus());
    }
  }, [open]);

  // Count matches when query changes
  useEffect(() => {
    if (!findQuery.trim()) {
      setMatchCount(0);
      setCurrentMatch(0);
      return;
    }
    const blocks = extractTextFromStore(editorStore);
    const fullText = blocks.join("\n");
    const query = findQuery.toLowerCase();
    let count = 0;
    let idx = 0;
    while (true) {
      idx = fullText.toLowerCase().indexOf(query, idx);
      if (idx === -1) break;
      count++;
      idx += query.length;
    }
    setMatchCount(count);
    setCurrentMatch(count > 0 ? 1 : 0);
  }, [findQuery, editorStore]);

  const handleNext = useCallback(() => {
    if (matchCount === 0) return;
    setCurrentMatch((prev) => (prev >= matchCount ? 1 : prev + 1));
  }, [matchCount]);

  const handlePrev = useCallback(() => {
    if (matchCount === 0) return;
    setCurrentMatch((prev) => (prev <= 1 ? matchCount : prev - 1));
  }, [matchCount]);

  const handleReplace = useCallback(() => {
    addToast({ message: "Replace coming soon", type: "info" });
  }, [addToast]);

  const handleReplaceAll = useCallback(() => {
    addToast({ message: "Replace coming soon", type: "info" });
  }, [addToast]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      } else if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        handleNext();
      } else if (e.key === "Enter" && e.shiftKey) {
        e.preventDefault();
        handlePrev();
      }
    },
    [onClose, handleNext, handlePrev]
  );

  if (!open) return null;

  return (
    <div
      style={{
        height: 40,
        width: "100%",
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "0 12px",
        background: "var(--celerex-bg-secondary, #f5f5f5)",
        borderBottom: "1px solid var(--celerex-border, #e5e7eb)",
        fontSize: 13,
        flexShrink: 0,
      }}
      onKeyDown={handleKeyDown}
    >
      {/* Find input */}
      <input
        ref={findInputRef}
        type="text"
        placeholder="Find..."
        value={findQuery}
        onChange={(e) => setFindQuery(e.target.value)}
        style={{
          width: 180,
          padding: "4px 8px",
          borderRadius: 4,
          border: "1px solid var(--celerex-border, #e5e7eb)",
          outline: "none",
          fontSize: 13,
          background: "var(--celerex-bg-primary, #fff)",
          color: "var(--celerex-text-primary, #111)",
        }}
      />

      {/* Replace input */}
      <input
        type="text"
        placeholder="Replace..."
        value={replaceQuery}
        onChange={(e) => setReplaceQuery(e.target.value)}
        style={{
          width: 180,
          padding: "4px 8px",
          borderRadius: 4,
          border: "1px solid var(--celerex-border, #e5e7eb)",
          outline: "none",
          fontSize: 13,
          background: "var(--celerex-bg-primary, #fff)",
          color: "var(--celerex-text-primary, #111)",
        }}
      />

      {/* Match count */}
      <span
        style={{
          color: "var(--celerex-text-secondary, #888)",
          fontSize: 12,
          minWidth: 90,
          textAlign: "center",
          userSelect: "none",
        }}
      >
        {findQuery.trim()
          ? matchCount > 0
            ? `${currentMatch} of ${matchCount} matches`
            : "No matches"
          : ""}
      </span>

      {/* Navigation buttons */}
      <button
        onClick={handlePrev}
        disabled={matchCount === 0}
        title="Previous match (Shift+Enter)"
        style={{
          padding: "2px 6px",
          borderRadius: 4,
          border: "1px solid var(--celerex-border, #e5e7eb)",
          background: "transparent",
          color: "var(--celerex-text-secondary, #888)",
          cursor: matchCount > 0 ? "pointer" : "not-allowed",
          fontSize: 13,
          lineHeight: 1,
          opacity: matchCount > 0 ? 1 : 0.4,
        }}
      >
        {"\u25B2"}
      </button>
      <button
        onClick={handleNext}
        disabled={matchCount === 0}
        title="Next match (Enter)"
        style={{
          padding: "2px 6px",
          borderRadius: 4,
          border: "1px solid var(--celerex-border, #e5e7eb)",
          background: "transparent",
          color: "var(--celerex-text-secondary, #888)",
          cursor: matchCount > 0 ? "pointer" : "not-allowed",
          fontSize: 13,
          lineHeight: 1,
          opacity: matchCount > 0 ? 1 : 0.4,
        }}
      >
        {"\u25BC"}
      </button>

      {/* Replace buttons */}
      <button
        onClick={handleReplace}
        disabled={matchCount === 0}
        title="Replace current match"
        style={{
          padding: "4px 8px",
          borderRadius: 4,
          border: "1px solid var(--celerex-border, #e5e7eb)",
          background: "transparent",
          color: "var(--celerex-text-secondary, #888)",
          cursor: matchCount > 0 ? "pointer" : "not-allowed",
          fontSize: 12,
          fontWeight: 500,
          opacity: matchCount > 0 ? 1 : 0.4,
        }}
      >
        Replace
      </button>
      <button
        onClick={handleReplaceAll}
        disabled={matchCount === 0}
        title="Replace all matches"
        style={{
          padding: "4px 8px",
          borderRadius: 4,
          border: "1px solid var(--celerex-border, #e5e7eb)",
          background: "transparent",
          color: "var(--celerex-text-secondary, #888)",
          cursor: matchCount > 0 ? "pointer" : "not-allowed",
          fontSize: 12,
          fontWeight: 500,
          opacity: matchCount > 0 ? 1 : 0.4,
        }}
      >
        Replace All
      </button>

      <div style={{ flex: 1 }} />

      {/* Close button */}
      <button
        onClick={onClose}
        title="Close (Esc)"
        style={{
          padding: "2px 6px",
          borderRadius: 4,
          border: "none",
          background: "transparent",
          color: "var(--celerex-text-secondary, #888)",
          cursor: "pointer",
          fontSize: 16,
          lineHeight: 1,
        }}
      >
        {"\u00D7"}
      </button>
    </div>
  );
}
