import { useAtomValue, useSetAtom } from "jotai";
import { activeViewAtom, activeDocAtom, engagementsAtom } from "../state/atoms";

export function Breadcrumb() {
  const activeView = useAtomValue(activeViewAtom);
  const activeDoc = useAtomValue(activeDocAtom);
  const engagements = useAtomValue(engagementsAtom);
  const setActiveView = useSetAtom(activeViewAtom);

  const segments: { label: string; onClick?: () => void }[] = [
    { label: "Celerex", onClick: () => setActiveView("editor") },
  ];

  switch (activeView) {
    case "editor": {
      if (activeDoc) {
        const engagement = engagements.find((e) =>
          e.docs.some((d) => d.id === activeDoc.id)
        );
        if (engagement) {
          segments.push({ label: engagement.name });
        }
        segments.push({ label: activeDoc.title });
      }
      break;
    }
    case "agent-dashboard":
      segments.push({ label: "Agents" });
      segments.push({ label: "Dashboard" });
      break;
    case "pipeline-editor":
      segments.push({ label: "Agents" });
      segments.push({ label: "Pipeline Editor" });
      break;
    case "task-queue":
      segments.push({ label: "Agents" });
      segments.push({ label: "Task Queue" });
      break;
    case "tools-manager":
      segments.push({ label: "Agents" });
      segments.push({ label: "Tools Manager" });
      break;
    case "agent-triggers":
      segments.push({ label: "Agents" });
      segments.push({ label: "Triggers" });
      break;
    case "plugin-manager":
      segments.push({ label: "Plugins" });
      segments.push({ label: "Manager" });
      break;
    case "settings":
      segments.push({ label: "Settings" });
      break;
  }

  return (
    <div
      style={{
        height: 28,
        display: "flex",
        alignItems: "center",
        paddingLeft: 16,
        gap: 6,
        fontSize: 12,
        color: "var(--celerex-text-secondary)",
        background: "var(--celerex-bg-secondary)",
        borderBottom: "1px solid var(--celerex-border)",
        userSelect: "none",
        flexShrink: 0,
      }}
    >
      {segments.map((seg, i) => (
        <span key={i} style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
          {i > 0 && (
            <span style={{ opacity: 0.5 }}>{"\u203A"}</span>
          )}
          {seg.onClick ? (
            <span
              onClick={seg.onClick}
              style={{
                cursor: "pointer",
                opacity: 0.7,
                transition: "opacity 0.15s",
              }}
              onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.opacity = "1"; }}
              onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.opacity = "0.7"; }}
            >
              {seg.label}
            </span>
          ) : (
            <span style={{ opacity: i === segments.length - 1 ? 1 : 0.7 }}>
              {seg.label}
            </span>
          )}
        </span>
      ))}
    </div>
  );
}
