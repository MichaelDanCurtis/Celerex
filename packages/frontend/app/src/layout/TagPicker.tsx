import { useAtomValue, useSetAtom } from "jotai";
import { activeDocIdAtom } from "../state/atoms";
import { tagsAtom, docTagsAtom, toggleDocTagAtom } from "../state/tagsAtom";

export function TagPicker() {
  const activeDocId = useAtomValue(activeDocIdAtom);
  const tags = useAtomValue(tagsAtom);
  const docTags = useAtomValue(docTagsAtom);
  const toggleDocTag = useSetAtom(toggleDocTagAtom);

  if (!activeDocId) return null;

  const activeTags = docTags[activeDocId] ?? [];

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
        Tags
      </span>
      {tags.map((tag) => {
        const isActive = activeTags.includes(tag.id);
        return (
          <button
            key={tag.id}
            onClick={() => toggleDocTag({ docId: activeDocId, tagId: tag.id })}
            style={{
              display: "inline-flex",
              alignItems: "center",
              padding: "1px 8px",
              fontSize: 11,
              lineHeight: "18px",
              borderRadius: 9999,
              border: `1px solid ${tag.color}`,
              background: isActive ? tag.color : "transparent",
              color: isActive ? "#fff" : tag.color,
              cursor: "pointer",
              fontWeight: 500,
              transition: "background 0.15s, color 0.15s",
              outline: "none",
            }}
          >
            {tag.name}
          </button>
        );
      })}
    </div>
  );
}
