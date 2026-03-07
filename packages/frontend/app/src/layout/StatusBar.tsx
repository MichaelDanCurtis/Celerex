import { useState, useEffect } from "react";
import { useAtomValue } from "jotai";
import { editorStoreAtom } from "../state/atoms";

interface StatusBarProps {
  mode: "page" | "edgeless";
}

function getDocStats(store: any): { words: number; chars: number } {
  if (!store?.root?.children) return { words: 0, chars: 0 };

  let fullText = "";
  for (const child of store.root.children) {
    const text = child.yBlock?.get("prop:text")?.toString();
    if (text) {
      fullText += text + "\n";
    }
  }

  const trimmed = fullText.trim();
  const chars = trimmed.length;
  const words = trimmed === "" ? 0 : trimmed.split(/\s+/).length;

  return { words, chars };
}

export function StatusBar({ mode }: StatusBarProps) {
  const store = useAtomValue(editorStoreAtom);
  const [stats, setStats] = useState({ words: 0, chars: 0 });

  useEffect(() => {
    if (!store) return;

    setStats(getDocStats(store));

    const id = setInterval(() => {
      setStats(getDocStats(store));
    }, 2000);

    return () => clearInterval(id);
  }, [store]);

  const modeLabel = mode === "edgeless" ? "Edgeless" : "Page";

  return (
    <div
      style={{
        height: 24,
        minHeight: 24,
        borderTop: "1px solid var(--celerex-border)",
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        padding: "0 12px",
        fontSize: 11,
        color: "var(--celerex-text-secondary)",
        background: "var(--celerex-bg-secondary)",
        userSelect: "none",
      }}
    >
      <div style={{ display: "flex", gap: 12 }}>
        {store ? (
          <>
            <span>{stats.words} words</span>
            <span>{stats.chars} characters</span>
          </>
        ) : (
          <span>No document</span>
        )}
      </div>
      <div>{modeLabel}</div>
    </div>
  );
}
