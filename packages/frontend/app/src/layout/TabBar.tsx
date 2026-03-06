import { useState } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import {
  openDocIdsAtom,
  activeDocIdAtom,
  closeDocAtom,
  engagementsAtom,
} from "../state/atoms";

export function TabBar() {
  const openDocIds = useAtomValue(openDocIdsAtom);
  const activeDocId = useAtomValue(activeDocIdAtom);
  const engagements = useAtomValue(engagementsAtom);
  const setActiveDocId = useSetAtom(activeDocIdAtom);
  const closeDoc = useSetAtom(closeDocAtom);

  // Build a lookup: docId -> title
  const titleMap = new Map<string, string>();
  for (const eng of engagements) {
    for (const doc of eng.docs) {
      titleMap.set(doc.id, doc.title || "Untitled");
    }
  }

  return (
    <div
      style={{
        height: 32,
        display: "flex",
        alignItems: "stretch",
        background: "var(--celerex-bg-sidebar)",
        borderBottom: "1px solid var(--celerex-border)",
        overflow: "hidden",
        flexShrink: 0,
      }}
    >
      {openDocIds.map((docId) => (
        <Tab
          key={docId}
          title={titleMap.get(docId) ?? "Untitled"}
          active={docId === activeDocId}
          closable={openDocIds.length > 1}
          onClick={() => setActiveDocId(docId)}
          onClose={() => closeDoc(docId)}
        />
      ))}
    </div>
  );
}

function Tab({
  title,
  active,
  closable,
  onClick,
  onClose,
}: {
  title: string;
  active: boolean;
  closable: boolean;
  onClick: () => void;
  onClose: () => void;
}) {
  const [hovered, setHovered] = useState(false);

  return (
    <div
      onClick={onClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 4,
        padding: "0 12px",
        fontSize: 12,
        cursor: "pointer",
        userSelect: "none",
        color: active
          ? "var(--celerex-text-primary)"
          : "var(--celerex-text-secondary)",
        fontWeight: active ? 500 : 400,
        borderBottom: active
          ? "2px solid var(--celerex-accent)"
          : "2px solid transparent",
        background: active
          ? "var(--celerex-bg-primary)"
          : "transparent",
        whiteSpace: "nowrap",
        maxWidth: 160,
      }}
    >
      <span
        style={{
          overflow: "hidden",
          textOverflow: "ellipsis",
        }}
      >
        {title}
      </span>
      {closable && hovered && (
        <span
          onClick={(e) => {
            e.stopPropagation();
            onClose();
          }}
          style={{
            fontSize: 14,
            lineHeight: 1,
            opacity: 0.5,
            cursor: "pointer",
            flexShrink: 0,
          }}
          title="Close tab"
        >
          ×
        </span>
      )}
    </div>
  );
}
