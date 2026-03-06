export function Sidebar() {
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
      {/* Logo / Brand */}
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

      {/* Workspace Nav */}
      <div style={{ flex: 1, padding: "12px 8px", overflow: "auto" }}>
        <SidebarSection title="Workspace">
          <SidebarItem label="All Pages" active />
          <SidebarItem label="Edgeless Canvas" />
          <SidebarItem label="Agent Fleet" />
        </SidebarSection>

        <SidebarSection title="Engagements">
          <SidebarItem label="New Engagement..." />
        </SidebarSection>

        <SidebarSection title="Agents">
          <SidebarItem label="Agent Dashboard" />
          <SidebarItem label="Task Queue" />
        </SidebarSection>

        <SidebarSection title="Plugins">
          <SidebarItem label="Plugin Manager" />
        </SidebarSection>
      </div>

      {/* Bottom Actions */}
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

function SidebarItem({ label, active }: { label: string; active?: boolean }) {
  return (
    <div
      style={{
        padding: "6px 8px",
        borderRadius: 6,
        fontSize: 13,
        cursor: "pointer",
        background: active ? "rgba(59, 130, 246, 0.08)" : "transparent",
        color: active ? "var(--celerex-accent)" : "var(--celerex-text-primary)",
        fontWeight: active ? 500 : 400,
      }}
    >
      {label}
    </div>
  );
}
