import { useState } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { activeDocIdAtom } from "../state/atoms";
import { docReactionsAtom, toggleReactionAtom } from "../state/reactionsAtom";

const PICKER_EMOJIS = ["\u{1F44D}", "\u{1F44E}", "\u2764\uFE0F", "\u{1F525}", "\u{1F4A1}", "\u{1F440}", "\u{1F389}", "\u{1F680}"];

export function ReactionBar() {
  const activeDocId = useAtomValue(activeDocIdAtom);
  const allReactions = useAtomValue(docReactionsAtom);
  const toggleReaction = useSetAtom(toggleReactionAtom);
  const [pickerOpen, setPickerOpen] = useState(false);

  if (!activeDocId) return null;

  const reactions = allReactions[activeDocId] ?? [];

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 6,
        padding: "4px 12px",
        borderBottom: "1px solid var(--celerex-border, #e5e7eb)",
        background: "var(--celerex-bg-primary, #fff)",
        minHeight: 28,
        position: "relative",
      }}
    >
      <span
        style={{
          fontSize: 11,
          color: "var(--celerex-text-secondary, #6b7280)",
          marginRight: 2,
          userSelect: "none",
        }}
      >
        Reactions
      </span>

      {reactions.map((reaction) => (
        <button
          key={reaction.emoji}
          onClick={() => toggleReaction({ docId: activeDocId, emoji: reaction.emoji })}
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 4,
            padding: "1px 8px",
            fontSize: 12,
            lineHeight: "18px",
            borderRadius: 9999,
            border: reaction.userReacted
              ? "1px solid var(--celerex-accent, #3b82f6)"
              : "1px solid var(--celerex-border, #d1d5db)",
            background: reaction.userReacted
              ? "var(--celerex-accent-light, rgba(59, 130, 246, 0.1))"
              : "transparent",
            color: "var(--celerex-text-primary, #1f2937)",
            cursor: "pointer",
            fontWeight: 500,
            transition: "background 0.15s, border-color 0.15s",
            outline: "none",
          }}
        >
          <span>{reaction.emoji}</span>
          <span style={{ fontSize: 11 }}>{reaction.count}</span>
        </button>
      ))}

      <div style={{ position: "relative" }}>
        <button
          onClick={() => setPickerOpen((o) => !o)}
          style={{
            display: "inline-flex",
            alignItems: "center",
            justifyContent: "center",
            width: 24,
            height: 20,
            fontSize: 13,
            borderRadius: 9999,
            border: "1px solid var(--celerex-border, #d1d5db)",
            background: "transparent",
            color: "var(--celerex-text-secondary, #6b7280)",
            cursor: "pointer",
            outline: "none",
            transition: "background 0.15s",
          }}
          title="Add reaction"
        >
          +
        </button>

        {pickerOpen && (
          <div
            style={{
              position: "absolute",
              top: "100%",
              left: 0,
              marginTop: 4,
              display: "flex",
              gap: 2,
              padding: "4px 6px",
              borderRadius: 8,
              border: "1px solid var(--celerex-border, #e5e7eb)",
              background: "var(--celerex-bg-secondary, #fff)",
              boxShadow: "0 4px 12px rgba(0,0,0,0.1)",
              zIndex: 50,
            }}
          >
            {PICKER_EMOJIS.map((emoji) => (
              <button
                key={emoji}
                onClick={() => {
                  toggleReaction({ docId: activeDocId, emoji });
                  setPickerOpen(false);
                }}
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  justifyContent: "center",
                  width: 28,
                  height: 28,
                  fontSize: 16,
                  border: "none",
                  borderRadius: 6,
                  background: "transparent",
                  cursor: "pointer",
                  transition: "background 0.1s",
                  outline: "none",
                }}
                onMouseEnter={(e) =>
                  (e.currentTarget.style.background =
                    "var(--celerex-bg-hover, #f3f4f6)")
                }
                onMouseLeave={(e) =>
                  (e.currentTarget.style.background = "transparent")
                }
              >
                {emoji}
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
