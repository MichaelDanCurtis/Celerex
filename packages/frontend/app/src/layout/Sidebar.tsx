import { useState, useRef, useEffect } from "react";
import { useAtom, useAtomValue } from "jotai";
import {
  engagementsAtom,
  activeDocIdAtom,
  genId,
  type Engagement,
} from "../state/atoms";

export function Sidebar() {
  const [engagements, setEngagements] = useAtom(engagementsAtom);
  const [activeDocId, setActiveDocId] = useAtom(activeDocIdAtom);
  const [creatingEngagement, setCreatingEngagement] = useState(false);

  function handleCreateEngagement(name: string) {
    const eng: Engagement = {
      id: genId("eng"),
      name,
      createdAt: Date.now(),
      docs: [],
    };
    setEngagements((prev) => [...prev, eng]);
    setCreatingEngagement(false);
  }

  function handleCreateDoc(engagementId: string, title: string) {
    const docId = genId("doc");
    setEngagements((prev) =>
      prev.map((eng) =>
        eng.id === engagementId
          ? {
              ...eng,
              docs: [
                ...eng.docs,
                { id: docId, title, engagementId, createdAt: Date.now() },
              ],
            }
          : eng
      )
    );
    setActiveDocId(docId);
  }

  function handleDeleteDoc(engagementId: string, docId: string) {
    setEngagements((prev) =>
      prev.map((eng) =>
        eng.id === engagementId
          ? { ...eng, docs: eng.docs.filter((d) => d.id !== docId) }
          : eng
      )
    );
    if (activeDocId === docId) {
      // Switch to first available doc
      const allDocs = engagements.flatMap((e) => e.docs).filter((d) => d.id !== docId);
      setActiveDocId(allDocs[0]?.id ?? "");
    }
  }

  function handleDeleteEngagement(engId: string) {
    const eng = engagements.find((e) => e.id === engId);
    if (!eng || eng.docs.length > 0) return; // Only delete empty engagements
    setEngagements((prev) => prev.filter((e) => e.id !== engId));
  }

  return (
    <aside
      style={{
        width: "var(--celerex-sidebar-width)",
        height: "100%",
        borderRight: "1px solid var(--celerex-border)",
        background: "var(--celerex-bg-sidebar)",
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
      }}
    >
      <div
        style={{
          height: 48,
          display: "flex",
          alignItems: "center",
          padding: "0 16px",
          borderBottom: "1px solid var(--celerex-border)",
          fontWeight: 700,
          fontSize: 16,
          letterSpacing: "-0.02em",
        }}
      >
        Celerex
      </div>

      <div style={{ flex: 1, padding: "12px 8px", overflow: "auto" }}>
        <SidebarSection title="Engagements">
          {engagements.map((eng) => (
            <EngagementGroup
              key={eng.id}
              engagement={eng}
              activeDocId={activeDocId}
              onSelectDoc={setActiveDocId}
              onCreateDoc={(title) => handleCreateDoc(eng.id, title)}
              onDeleteDoc={(docId) => handleDeleteDoc(eng.id, docId)}
              onDeleteEngagement={() => handleDeleteEngagement(eng.id)}
            />
          ))}
          {creatingEngagement ? (
            <InlineInput
              placeholder="Engagement name..."
              onSubmit={handleCreateEngagement}
              onCancel={() => setCreatingEngagement(false)}
            />
          ) : (
            <SidebarItem
              label="+ New Engagement"
              muted
              onClick={() => setCreatingEngagement(true)}
            />
          )}
        </SidebarSection>

        <SidebarSection title="Agents">
          <SidebarItem label="Agent Dashboard" />
          <SidebarItem label="Task Queue" />
        </SidebarSection>

        <SidebarSection title="Plugins">
          <SidebarItem label="Plugin Manager" />
        </SidebarSection>
      </div>

      <div
        style={{
          padding: "12px 16px",
          borderTop: "1px solid var(--celerex-border)",
          fontSize: 12,
          color: "var(--celerex-text-secondary)",
        }}
      >
        Celerex v0.1.0
      </div>
    </aside>
  );
}

function EngagementGroup({
  engagement,
  activeDocId,
  onSelectDoc,
  onCreateDoc,
  onDeleteDoc,
  onDeleteEngagement,
}: {
  engagement: Engagement;
  activeDocId: string;
  onSelectDoc: (id: string) => void;
  onCreateDoc: (title: string) => void;
  onDeleteDoc: (docId: string) => void;
  onDeleteEngagement: () => void;
}) {
  const [collapsed, setCollapsed] = useState(false);
  const [creatingDoc, setCreatingDoc] = useState(false);

  return (
    <div style={{ marginBottom: 4 }}>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          padding: "4px 8px",
          borderRadius: 6,
          cursor: "pointer",
          fontSize: 13,
          fontWeight: 500,
          color: "var(--celerex-text-primary)",
        }}
      >
        <span
          onClick={() => setCollapsed(!collapsed)}
          style={{
            marginRight: 4,
            fontSize: 10,
            width: 16,
            textAlign: "center",
            userSelect: "none",
          }}
        >
          {collapsed ? "▸" : "▾"}
        </span>
        <span onClick={() => setCollapsed(!collapsed)} style={{ flex: 1 }}>
          {engagement.name}
        </span>
        <span
          onClick={(e) => {
            e.stopPropagation();
            setCreatingDoc(true);
          }}
          style={{
            fontSize: 16,
            lineHeight: 1,
            opacity: 0.4,
            padding: "0 2px",
            cursor: "pointer",
          }}
          title="New doc"
        >
          +
        </span>
      </div>
      {!collapsed && (
        <div style={{ paddingLeft: 12 }}>
          {engagement.docs.map((doc) => (
            <SidebarItem
              key={doc.id}
              label={doc.title || "Untitled"}
              active={doc.id === activeDocId}
              onClick={() => onSelectDoc(doc.id)}
            />
          ))}
          {creatingDoc && (
            <InlineInput
              placeholder="Doc title..."
              onSubmit={(title) => {
                onCreateDoc(title || "Untitled");
                setCreatingDoc(false);
              }}
              onCancel={() => setCreatingDoc(false)}
            />
          )}
        </div>
      )}
    </div>
  );
}

function InlineInput({
  placeholder,
  onSubmit,
  onCancel,
}: {
  placeholder: string;
  onSubmit: (value: string) => void;
  onCancel: () => void;
}) {
  const ref = useRef<HTMLInputElement>(null);

  useEffect(() => {
    ref.current?.focus();
  }, []);

  return (
    <input
      ref={ref}
      placeholder={placeholder}
      style={{
        display: "block",
        width: "100%",
        padding: "5px 8px",
        fontSize: 13,
        border: "1px solid var(--celerex-accent)",
        borderRadius: 6,
        outline: "none",
        background: "var(--celerex-bg-primary)",
        color: "var(--celerex-text-primary)",
        boxSizing: "border-box",
        margin: "2px 0",
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          const val = (e.target as HTMLInputElement).value.trim();
          if (val) onSubmit(val);
        }
        if (e.key === "Escape") onCancel();
      }}
      onBlur={(e) => {
        const val = e.target.value.trim();
        if (val) onSubmit(val);
        else onCancel();
      }}
    />
  );
}

function SidebarSection({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div style={{ marginBottom: 16 }}>
      <div
        style={{
          fontSize: 11,
          fontWeight: 600,
          textTransform: "uppercase",
          letterSpacing: "0.05em",
          color: "var(--celerex-text-secondary)",
          padding: "4px 8px",
          marginBottom: 2,
        }}
      >
        {title}
      </div>
      {children}
    </div>
  );
}

function SidebarItem({
  label,
  active,
  muted,
  onClick,
}: {
  label: string;
  active?: boolean;
  muted?: boolean;
  onClick?: () => void;
}) {
  return (
    <div
      onClick={onClick}
      style={{
        padding: "5px 8px",
        borderRadius: 6,
        fontSize: 13,
        cursor: onClick ? "pointer" : "default",
        background: active ? "rgba(59, 130, 246, 0.08)" : "transparent",
        color: active
          ? "var(--celerex-accent)"
          : muted
            ? "var(--celerex-text-secondary)"
            : "var(--celerex-text-primary)",
        fontWeight: active ? 500 : 400,
      }}
    >
      {label}
    </div>
  );
}
