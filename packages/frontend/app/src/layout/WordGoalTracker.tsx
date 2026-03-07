import { useState, useEffect, useRef } from "react";
import { useAtomValue } from "jotai";
import { editorStoreAtom } from "../state/atoms";

function getWordCount(store: any): number {
  if (!store?.root?.children) return 0;

  let fullText = "";
  for (const child of store.root.children) {
    const text = child.yBlock?.get("prop:text")?.toString();
    if (text) {
      fullText += text + "\n";
    }
  }

  const trimmed = fullText.trim();
  return trimmed === "" ? 0 : trimmed.split(/\s+/).length;
}

export function WordGoalTracker() {
  const store = useAtomValue(editorStoreAtom);
  const [wordCount, setWordCount] = useState(0);
  const [goal, setGoal] = useState(500);
  const [editing, setEditing] = useState(false);
  const [inputValue, setInputValue] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!store) return;

    setWordCount(getWordCount(store));

    const id = setInterval(() => {
      setWordCount(getWordCount(store));
    }, 2000);

    return () => clearInterval(id);
  }, [store]);

  useEffect(() => {
    if (editing && inputRef.current) {
      inputRef.current.focus();
      inputRef.current.select();
    }
  }, [editing]);

  const percentage = goal > 0 ? Math.min((wordCount / goal) * 100, 100) : 0;

  let barColor = "var(--celerex-text-secondary, #999)";
  if (percentage >= 75) {
    barColor = "#4caf50";
  } else if (percentage >= 50) {
    barColor = "#ffb300";
  }

  function handleSetGoal() {
    setInputValue(String(goal));
    setEditing(true);
  }

  function commitGoal() {
    const parsed = parseInt(inputValue, 10);
    if (parsed > 0) {
      setGoal(parsed);
    }
    setEditing(false);
  }

  if (!store) return null;

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "3px 12px",
        fontSize: 11,
        color: "var(--celerex-text-secondary)",
        background: "var(--celerex-bg-secondary)",
        borderBottom: "1px solid var(--celerex-border)",
        userSelect: "none",
        minHeight: 22,
      }}
    >
      <span style={{ whiteSpace: "nowrap" }}>
        {wordCount} / {goal} words
      </span>

      <div
        style={{
          flex: 1,
          maxWidth: 120,
          height: 6,
          borderRadius: 3,
          background: "var(--celerex-border, #ddd)",
          overflow: "hidden",
        }}
      >
        <div
          style={{
            width: `${percentage}%`,
            height: "100%",
            borderRadius: 3,
            background: barColor,
            transition: "width 0.3s ease, background 0.3s ease",
          }}
        />
      </div>

      <span style={{ fontSize: 10, whiteSpace: "nowrap" }}>
        {Math.round(percentage)}%
      </span>

      {editing ? (
        <input
          ref={inputRef}
          type="number"
          min={1}
          value={inputValue}
          onChange={(e) => setInputValue(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") commitGoal();
            if (e.key === "Escape") setEditing(false);
          }}
          onBlur={commitGoal}
          style={{
            width: 60,
            height: 16,
            fontSize: 11,
            padding: "0 4px",
            border: "1px solid var(--celerex-border)",
            borderRadius: 3,
            background: "var(--celerex-bg-primary)",
            color: "var(--celerex-text-primary)",
            outline: "none",
          }}
        />
      ) : (
        <button
          onClick={handleSetGoal}
          style={{
            fontSize: 10,
            padding: "1px 6px",
            border: "1px solid var(--celerex-border)",
            borderRadius: 3,
            background: "var(--celerex-bg-primary)",
            color: "var(--celerex-text-secondary)",
            cursor: "pointer",
            lineHeight: "14px",
          }}
        >
          Set Goal
        </button>
      )}
    </div>
  );
}
