import { useState, useEffect } from "react";
import { useAtomValue } from "jotai";
import { editorStoreAtom } from "../state/atoms";

interface StatusBarProps {
  mode: "page" | "edgeless";
}

export function StatusBar({ mode }: StatusBarProps) {
  const store = useAtomValue(editorStoreAtom);
  const [wordCount, setWordCount] = useState(0);
  const [charCount, setCharCount] = useState(0);

  useEffect(() => {
    function update() {
      if (!store?.root) {
        setWordCount(0);
        setCharCount(0);
        return;
      }
      const children = store.root.children ?? [];
      const texts: string[] = [];
      for (const child of children) {
        const text = child.yBlock?.get("prop:text")?.toString();
        if (text) texts.push(text);
      }
      const fullText = texts.join(" ");
      const words = fullText.split(/\s+/).filter(Boolean);
      setWordCount(words.length);
      setCharCount(fullText.length);
    }

    update();
    const id = setInterval(update, 1000);
    return () => clearInterval(id);
  }, [store]);

  const modeLabel = mode === "edgeless" ? "Edgeless" : "Page";

  return (
    <div
      style={{
        height: 24,
        minHeight: 24,
        borderTop: "1px solid var(--celerex-border, #e0e0e0)",
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        padding: "0 12px",
        fontSize: 11,
        color: "var(--celerex-text-secondary, #888)",
        background: "var(--celerex-bg-primary, #fff)",
        userSelect: "none",
      }}
    >
      <span>{modeLabel}</span>
      <span>
        {wordCount} word{wordCount !== 1 ? "s" : ""} &middot;{" "}
        {charCount} character{charCount !== 1 ? "s" : ""}
      </span>
    </div>
  );
}
