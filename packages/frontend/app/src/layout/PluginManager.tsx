import { useState } from "react";

interface InstalledPlugin {
  name: string;
  version: string;
  enabled: boolean;
  description: string;
}

interface AvailablePlugin {
  name: string;
  description: string;
}

const initialInstalled: InstalledPlugin[] = [
  {
    name: "Diagram Generator",
    version: "1.2.0",
    enabled: true,
    description:
      "Generates Mermaid and PlantUML diagrams from descriptions",
  },
  {
    name: "Citation Manager",
    version: "0.8.1",
    enabled: true,
    description:
      "Manages references and citations for research documents",
  },
  {
    name: "Template Library",
    version: "2.0.0",
    enabled: false,
    description:
      "Pre-built document templates for common consulting deliverables",
  },
];

const availablePlugins: AvailablePlugin[] = [
  {
    name: "Data Visualizer",
    description: "Create charts and graphs from CSV/JSON data",
  },
  {
    name: "Version History",
    description: "Track document changes with diff view",
  },
];

export function PluginManager() {
  const [installed, setInstalled] = useState(initialInstalled);

  const handleToggle = (name: string) => {
    setInstalled((prev) =>
      prev.map((p) =>
        p.name === name ? { ...p, enabled: !p.enabled } : p
      )
    );
  };

  return (
    <div
      style={{
        flex: 1,
        height: "100%",
        overflow: "auto",
        background: "var(--celerex-bg-primary)",
      }}
    >
      <div style={{ maxWidth: 800, margin: "0 auto", padding: "32px 24px" }}>
        <h1
          style={{
            margin: 0,
            fontSize: 22,
            fontWeight: 700,
            color: "var(--celerex-text-primary)",
          }}
        >
          Plugin Manager
        </h1>
        <p
          style={{
            margin: "4px 0 24px",
            fontSize: 14,
            color: "var(--celerex-text-secondary)",
          }}
        >
          Extend Celerex with plugins
        </p>

        {/* Installed Plugins */}
        <h2
          style={{
            margin: "0 0 12px",
            fontSize: 16,
            fontWeight: 600,
            color: "var(--celerex-text-primary)",
          }}
        >
          Installed Plugins
        </h2>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fill, minmax(340px, 1fr))",
            gap: 16,
          }}
        >
          {installed.map((plugin) => (
            <InstalledPluginCard
              key={plugin.name}
              plugin={plugin}
              onToggle={() => handleToggle(plugin.name)}
            />
          ))}
        </div>

        {/* Available Plugins */}
        <h2
          style={{
            margin: "32px 0 12px",
            fontSize: 16,
            fontWeight: 600,
            color: "var(--celerex-text-primary)",
          }}
        >
          Available Plugins
        </h2>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fill, minmax(340px, 1fr))",
            gap: 16,
          }}
        >
          {availablePlugins.map((plugin) => (
            <AvailablePluginCard key={plugin.name} plugin={plugin} />
          ))}
        </div>
      </div>
    </div>
  );
}

function ToggleSwitch({
  enabled,
  onToggle,
}: {
  enabled: boolean;
  onToggle: () => void;
}) {
  return (
    <button
      role="switch"
      aria-checked={enabled}
      onClick={onToggle}
      style={{
        position: "relative",
        width: 40,
        height: 22,
        borderRadius: 11,
        border: "none",
        cursor: "pointer",
        background: enabled ? "var(--celerex-accent)" : "var(--celerex-border)",
        transition: "background 0.2s ease",
        padding: 0,
        flexShrink: 0,
      }}
    >
      <span
        style={{
          position: "absolute",
          top: 2,
          left: enabled ? 20 : 2,
          width: 18,
          height: 18,
          borderRadius: "50%",
          background: "#fff",
          transition: "left 0.2s ease",
          boxShadow: "0 1px 3px rgba(0,0,0,0.2)",
        }}
      />
    </button>
  );
}

function InstalledPluginCard({
  plugin,
  onToggle,
}: {
  plugin: InstalledPlugin;
  onToggle: () => void;
}) {
  return (
    <div
      style={{
        padding: 16,
        borderRadius: 8,
        border: "1px solid var(--celerex-border)",
        background: "var(--celerex-bg-sidebar)",
        display: "flex",
        flexDirection: "column",
        gap: 8,
        opacity: plugin.enabled ? 1 : 0.7,
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span
            style={{
              fontSize: 15,
              fontWeight: 600,
              color: "var(--celerex-text-primary)",
            }}
          >
            {plugin.name}
          </span>
          <span
            style={{
              fontSize: 11,
              color: "var(--celerex-text-secondary)",
              background: "var(--celerex-bg-primary)",
              padding: "1px 6px",
              borderRadius: 4,
              fontWeight: 500,
            }}
          >
            v{plugin.version}
          </span>
        </div>
        <ToggleSwitch enabled={plugin.enabled} onToggle={onToggle} />
      </div>
      <p
        style={{
          margin: 0,
          fontSize: 13,
          lineHeight: 1.45,
          color: "var(--celerex-text-secondary)",
        }}
      >
        {plugin.description}
      </p>
      <div
        style={{
          marginTop: 4,
          fontSize: 12,
          color: plugin.enabled ? "#22c55e" : "#9ca3af",
          fontWeight: 500,
        }}
      >
        {plugin.enabled ? "Enabled" : "Disabled"}
      </div>
    </div>
  );
}

function AvailablePluginCard({ plugin }: { plugin: AvailablePlugin }) {
  return (
    <div
      style={{
        padding: 16,
        borderRadius: 8,
        border: "1px solid var(--celerex-border)",
        background: "var(--celerex-bg-sidebar)",
        display: "flex",
        flexDirection: "column",
        gap: 8,
      }}
    >
      <span
        style={{
          fontSize: 15,
          fontWeight: 600,
          color: "var(--celerex-text-primary)",
        }}
      >
        {plugin.name}
      </span>
      <p
        style={{
          margin: 0,
          fontSize: 13,
          lineHeight: 1.45,
          color: "var(--celerex-text-secondary)",
        }}
      >
        {plugin.description}
      </p>
      <div style={{ marginTop: 4 }}>
        <button
          onClick={() => {}}
          style={{
            padding: "5px 14px",
            borderRadius: 6,
            border: "none",
            background: "var(--celerex-accent)",
            color: "#fff",
            fontSize: 12,
            fontWeight: 500,
            cursor: "pointer",
          }}
        >
          Install
        </button>
      </div>
    </div>
  );
}
