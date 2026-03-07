import { useState, useMemo } from "react";
import {
  getAllTools,
  TOOL_CATEGORIES,
  EXECUTION_TYPES,
  type ToolDef,
  type ExecutionType,
  type ExecutionConfig,
} from "../data/tools";

/* ------------------------------------------------------------------ */
/*  Helpers                                                           */
/* ------------------------------------------------------------------ */

/** A "plugin" is any tool that has an execution backend configured. */
function getPlugins(): ToolDef[] {
  return getAllTools().filter((t) => t.execution != null);
}

function getExecMeta(type: ExecutionType) {
  return EXECUTION_TYPES.find((e) => e.id === type);
}

function getCategoryColor(catId: string) {
  return TOOL_CATEGORIES.find((c) => c.id === catId)?.color ?? "#6b7280";
}

function getCategoryLabel(catId: string) {
  return TOOL_CATEGORIES.find((c) => c.id === catId)?.label ?? catId;
}

/** Return a short summary string for the key config of an execution backend. */
function configSummary(exec: ExecutionConfig): string {
  switch (exec.type) {
    case "mcp": {
      const m = exec.mcp;
      if (!m) return "No MCP config";
      if (m.transport === "stdio" && m.command) return `${m.command} ${(m.args ?? []).join(" ")}`;
      return m.serverUrl || "No server URL";
    }
    case "http": {
      const h = exec.http;
      if (!h) return "No HTTP config";
      return `${h.method} ${h.url}`;
    }
    case "agent": {
      const a = exec.agent;
      if (!a) return "No agent config";
      return `Target: ${a.agentId || "(none)"}`;
    }
    case "script": {
      const s = exec.script;
      if (!s) return "No script config";
      return `${s.command} ${(s.args ?? []).join(" ")}`;
    }
    case "sandbox": {
      const sb = exec.sandbox;
      if (!sb) return "No sandbox config";
      return `${sb.image} — ${sb.command}`;
    }
    default:
      return "";
  }
}

/** Determine plugin status. */
function pluginStatus(exec: ExecutionConfig): "connected" | "configured" | "needs-setup" {
  switch (exec.type) {
    case "mcp": {
      const m = exec.mcp;
      if (!m) return "needs-setup";
      if (m.transport === "stdio" && !m.command) return "needs-setup";
      if (m.transport !== "stdio" && !m.serverUrl) return "needs-setup";
      // Check for empty env values that look like required keys
      if (m.env) {
        for (const v of Object.values(m.env)) {
          if (v === "") return "needs-setup";
        }
      }
      return "configured";
    }
    case "http": {
      const h = exec.http;
      if (!h || !h.url) return "needs-setup";
      if (h.authType && h.authType !== "none" && !h.authValue) return "needs-setup";
      return "configured";
    }
    case "agent": {
      const a = exec.agent;
      if (!a || !a.agentId) return "needs-setup";
      return "configured";
    }
    case "script": {
      const s = exec.script;
      if (!s || !s.command) return "needs-setup";
      return "configured";
    }
    case "sandbox": {
      const sb = exec.sandbox;
      if (!sb || !sb.image || !sb.command) return "needs-setup";
      return "configured";
    }
    default:
      return "needs-setup";
  }
}

const STATUS_STYLES: Record<string, { color: string; bg: string; label: string }> = {
  connected: { color: "#22c55e", bg: "rgba(34,197,94,0.12)", label: "Connected" },
  configured: { color: "#3b82f6", bg: "rgba(59,130,246,0.12)", label: "Configured" },
  "needs-setup": { color: "#f59e0b", bg: "rgba(245,158,11,0.12)", label: "Needs Setup" },
};

/* ------------------------------------------------------------------ */
/*  Detail Panel                                                      */
/* ------------------------------------------------------------------ */

function DetailSection({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div style={{ marginBottom: 16 }}>
      <div
        style={{
          fontSize: 11,
          fontWeight: 600,
          textTransform: "uppercase",
          letterSpacing: "0.05em",
          color: "var(--celerex-text-secondary)",
          marginBottom: 6,
        }}
      >
        {label}
      </div>
      {children}
    </div>
  );
}

function ConfigRow({ label, value }: { label: string; value: string | undefined }) {
  return (
    <div style={{ display: "flex", gap: 8, fontSize: 13, lineHeight: 1.6 }}>
      <span style={{ color: "var(--celerex-text-secondary)", minWidth: 110, flexShrink: 0 }}>
        {label}:
      </span>
      <span
        style={{
          color: "var(--celerex-text-primary)",
          fontFamily: "monospace",
          wordBreak: "break-all",
        }}
      >
        {value || "(empty)"}
      </span>
    </div>
  );
}

function PluginDetail({
  plugin,
  onClose,
}: {
  plugin: ToolDef;
  onClose: () => void;
}) {
  const exec = plugin.execution!;
  const meta = getExecMeta(exec.type);
  const status = pluginStatus(exec);
  const st = STATUS_STYLES[status];

  const renderConfig = () => {
    switch (exec.type) {
      case "mcp": {
        const m = exec.mcp;
        if (!m) return null;
        return (
          <>
            <ConfigRow label="Transport" value={m.transport} />
            {m.transport === "stdio" ? (
              <>
                <ConfigRow label="Command" value={m.command} />
                <ConfigRow label="Args" value={(m.args ?? []).join(" ")} />
              </>
            ) : (
              <ConfigRow label="Server URL" value={m.serverUrl} />
            )}
            <ConfigRow label="Tool Name" value={m.toolName} />
            {m.env && Object.keys(m.env).length > 0 && (
              <DetailSection label="Environment Variables">
                {Object.entries(m.env).map(([k, v]) => (
                  <ConfigRow key={k} label={k} value={v || "(not set)"} />
                ))}
              </DetailSection>
            )}
          </>
        );
      }
      case "http": {
        const h = exec.http;
        if (!h) return null;
        return (
          <>
            <ConfigRow label="URL" value={h.url} />
            <ConfigRow label="Method" value={h.method} />
            <ConfigRow label="Auth Type" value={h.authType ?? "none"} />
            {h.authType && h.authType !== "none" && (
              <ConfigRow label="Auth Value" value={h.authValue ? "(set)" : "(empty)"} />
            )}
            {h.authHeader && <ConfigRow label="Auth Header" value={h.authHeader} />}
            {h.bodyTemplate && <ConfigRow label="Body Tpl" value={h.bodyTemplate} />}
          </>
        );
      }
      case "agent": {
        const a = exec.agent;
        if (!a) return null;
        return (
          <>
            <ConfigRow label="Agent ID" value={a.agentId} />
            {a.inputMapping && Object.keys(a.inputMapping).length > 0 && (
              <DetailSection label="Input Mapping">
                {Object.entries(a.inputMapping).map(([k, v]) => (
                  <ConfigRow key={k} label={k} value={v} />
                ))}
              </DetailSection>
            )}
          </>
        );
      }
      case "script": {
        const s = exec.script;
        if (!s) return null;
        return (
          <>
            <ConfigRow label="Command" value={s.command} />
            <ConfigRow label="Args" value={(s.args ?? []).join(" ")} />
            {s.cwd && <ConfigRow label="CWD" value={s.cwd} />}
            {s.timeout != null && <ConfigRow label="Timeout" value={`${s.timeout}s`} />}
          </>
        );
      }
      case "sandbox": {
        const sb = exec.sandbox;
        if (!sb) return null;
        return (
          <>
            <ConfigRow label="Image" value={sb.image} />
            <ConfigRow label="Command" value={sb.command} />
            {sb.timeout != null && <ConfigRow label="Timeout" value={`${sb.timeout}s`} />}
            {sb.mounts && sb.mounts.length > 0 && (
              <ConfigRow label="Mounts" value={sb.mounts.join(", ")} />
            )}
          </>
        );
      }
      default:
        return null;
    }
  };

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 1000,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "rgba(0,0,0,0.45)",
      }}
      onClick={onClose}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          width: 560,
          maxHeight: "80vh",
          overflow: "auto",
          background: "var(--celerex-bg-sidebar)",
          border: "1px solid var(--celerex-border)",
          borderRadius: 12,
          padding: 28,
          boxShadow: "0 16px 48px rgba(0,0,0,0.25)",
        }}
      >
        {/* Close button */}
        <div style={{ display: "flex", justifyContent: "flex-end" }}>
          <button
            onClick={onClose}
            style={{
              background: "none",
              border: "none",
              fontSize: 18,
              cursor: "pointer",
              color: "var(--celerex-text-secondary)",
              padding: 0,
              lineHeight: 1,
            }}
            aria-label="Close"
          >
            &times;
          </button>
        </div>

        {/* Title + badges */}
        <h2
          style={{
            margin: "0 0 8px",
            fontSize: 20,
            fontWeight: 700,
            color: "var(--celerex-text-primary)",
          }}
        >
          {plugin.name}
        </h2>
        <div style={{ display: "flex", gap: 8, flexWrap: "wrap", marginBottom: 16 }}>
          {/* Category badge */}
          <span
            style={{
              fontSize: 11,
              fontWeight: 600,
              padding: "2px 8px",
              borderRadius: 4,
              color: getCategoryColor(plugin.category),
              background: `${getCategoryColor(plugin.category)}18`,
            }}
          >
            {getCategoryLabel(plugin.category)}
          </span>
          {/* Exec type badge */}
          <span
            style={{
              fontSize: 11,
              fontWeight: 600,
              padding: "2px 8px",
              borderRadius: 4,
              color: "var(--celerex-text-secondary)",
              background: "var(--celerex-bg-primary)",
            }}
          >
            {meta?.icon} {meta?.label}
          </span>
          {/* Status badge */}
          <span
            style={{
              fontSize: 11,
              fontWeight: 600,
              padding: "2px 8px",
              borderRadius: 4,
              color: st.color,
              background: st.bg,
            }}
          >
            {st.label}
          </span>
        </div>

        <p
          style={{
            margin: "0 0 20px",
            fontSize: 13,
            lineHeight: 1.55,
            color: "var(--celerex-text-secondary)",
          }}
        >
          {plugin.description}
        </p>

        <DetailSection label="Execution Configuration">{renderConfig()}</DetailSection>

        {/* Parameters */}
        {plugin.parameters.length > 0 && (
          <DetailSection label="Parameters">
            {plugin.parameters.map((p) => (
              <div
                key={p.name}
                style={{
                  fontSize: 13,
                  lineHeight: 1.6,
                  display: "flex",
                  gap: 6,
                }}
              >
                <code
                  style={{
                    color: "var(--celerex-accent)",
                    fontFamily: "monospace",
                    fontSize: 12,
                  }}
                >
                  {p.name}
                </code>
                <span style={{ color: "var(--celerex-text-secondary)" }}>
                  ({p.type}){p.required ? " *" : ""} — {p.description}
                </span>
              </div>
            ))}
          </DetailSection>
        )}
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Plugin Card                                                       */
/* ------------------------------------------------------------------ */

function PluginCard({
  plugin,
  onClick,
}: {
  plugin: ToolDef;
  onClick: () => void;
}) {
  const exec = plugin.execution!;
  const meta = getExecMeta(exec.type);
  const status = pluginStatus(exec);
  const st = STATUS_STYLES[status];

  return (
    <div
      onClick={onClick}
      style={{
        padding: 16,
        borderRadius: 8,
        border: "1px solid var(--celerex-border)",
        background: "var(--celerex-bg-sidebar)",
        cursor: "pointer",
        transition: "border-color 0.15s, box-shadow 0.15s",
        display: "flex",
        flexDirection: "column",
        gap: 8,
      }}
      onMouseEnter={(e) => {
        (e.currentTarget as HTMLElement).style.borderColor = "var(--celerex-accent)";
        (e.currentTarget as HTMLElement).style.boxShadow =
          "0 0 0 1px var(--celerex-accent)";
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLElement).style.borderColor = "var(--celerex-border)";
        (e.currentTarget as HTMLElement).style.boxShadow = "none";
      }}
    >
      {/* Top row: name + status */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 8,
        }}
      >
        <span
          style={{
            fontSize: 15,
            fontWeight: 600,
            color: "var(--celerex-text-primary)",
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {plugin.name}
        </span>
        <span
          style={{
            fontSize: 11,
            fontWeight: 600,
            padding: "2px 8px",
            borderRadius: 4,
            color: st.color,
            background: st.bg,
            whiteSpace: "nowrap",
            flexShrink: 0,
          }}
        >
          {st.label}
        </span>
      </div>

      {/* Description */}
      <p
        style={{
          margin: 0,
          fontSize: 13,
          lineHeight: 1.45,
          color: "var(--celerex-text-secondary)",
          display: "-webkit-box",
          WebkitLineClamp: 2,
          WebkitBoxOrient: "vertical",
          overflow: "hidden",
        }}
      >
        {plugin.description}
      </p>

      {/* Badges row: category + exec type */}
      <div style={{ display: "flex", gap: 6, alignItems: "center", flexWrap: "wrap" }}>
        <span
          style={{
            fontSize: 11,
            fontWeight: 600,
            padding: "2px 8px",
            borderRadius: 4,
            color: getCategoryColor(plugin.category),
            background: `${getCategoryColor(plugin.category)}18`,
          }}
        >
          {getCategoryLabel(plugin.category)}
        </span>
        <span
          style={{
            fontSize: 11,
            fontWeight: 500,
            padding: "2px 8px",
            borderRadius: 4,
            color: "var(--celerex-text-secondary)",
            background: "var(--celerex-bg-primary)",
          }}
        >
          {meta?.icon} {meta?.label}
        </span>
      </div>

      {/* Config summary */}
      <div
        style={{
          fontSize: 12,
          fontFamily: "monospace",
          color: "var(--celerex-text-secondary)",
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
          marginTop: 2,
        }}
      >
        {configSummary(exec)}
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Main Component                                                    */
/* ------------------------------------------------------------------ */

export function PluginManager() {
  const [execFilter, setExecFilter] = useState<ExecutionType | "all">("all");
  const [selectedPlugin, setSelectedPlugin] = useState<ToolDef | null>(null);

  const plugins = useMemo(() => getPlugins(), []);

  const filtered = useMemo(() => {
    if (execFilter === "all") return plugins;
    return plugins.filter((p) => p.execution!.type === execFilter);
  }, [plugins, execFilter]);

  /** Group filtered plugins by execution type. */
  const grouped = useMemo(() => {
    const map = new Map<ExecutionType, ToolDef[]>();
    for (const p of filtered) {
      const t = p.execution!.type;
      if (!map.has(t)) map.set(t, []);
      map.get(t)!.push(p);
    }
    // Sort groups in the canonical EXECUTION_TYPES order
    const ordered: { type: ExecutionType; tools: ToolDef[] }[] = [];
    for (const et of EXECUTION_TYPES) {
      const list = map.get(et.id);
      if (list && list.length > 0) ordered.push({ type: et.id, tools: list });
    }
    return ordered;
  }, [filtered]);

  /** Counts per execution type for the filter pills. */
  const countsByType = useMemo(() => {
    const c: Record<string, number> = {};
    for (const p of plugins) c[p.execution!.type] = (c[p.execution!.type] || 0) + 1;
    return c;
  }, [plugins]);

  return (
    <div
      style={{
        flex: 1,
        height: "100%",
        overflow: "auto",
        background: "var(--celerex-bg-primary)",
      }}
    >
      <div style={{ maxWidth: 1100, margin: "0 auto", padding: "32px 24px" }}>
        {/* Header */}
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
            margin: "4px 0 0",
            fontSize: 14,
            color: "var(--celerex-text-secondary)",
          }}
        >
          Tools with execution backends &mdash;{" "}
          <strong style={{ color: "var(--celerex-text-primary)" }}>{plugins.length}</strong>{" "}
          plugin{plugins.length !== 1 ? "s" : ""} total
        </p>

        {/* CTA banner */}
        <div
          style={{
            marginTop: 20,
            padding: "12px 16px",
            borderRadius: 8,
            border: "1px solid var(--celerex-accent)",
            background: "var(--celerex-accent)0a",
            fontSize: 13,
            color: "var(--celerex-text-secondary)",
            lineHeight: 1.5,
          }}
        >
          Create new plugins in the{" "}
          <strong style={{ color: "var(--celerex-accent)" }}>Tools Manager</strong> by adding
          an execution backend to any tool.
        </div>

        {/* Execution type filter pills */}
        <div
          style={{
            display: "flex",
            gap: 8,
            flexWrap: "wrap",
            marginTop: 20,
            marginBottom: 24,
          }}
        >
          {/* "All" pill */}
          <button
            onClick={() => setExecFilter("all")}
            style={{
              padding: "5px 14px",
              borderRadius: 16,
              border: "1px solid",
              borderColor:
                execFilter === "all" ? "var(--celerex-accent)" : "var(--celerex-border)",
              background:
                execFilter === "all" ? "var(--celerex-accent)" : "transparent",
              color: execFilter === "all" ? "#fff" : "var(--celerex-text-secondary)",
              fontSize: 12,
              fontWeight: 600,
              cursor: "pointer",
              transition: "all 0.15s",
            }}
          >
            All ({plugins.length})
          </button>
          {EXECUTION_TYPES.map((et) => {
            const count = countsByType[et.id] || 0;
            if (count === 0) return null;
            const active = execFilter === et.id;
            return (
              <button
                key={et.id}
                onClick={() => setExecFilter(active ? "all" : et.id)}
                style={{
                  padding: "5px 14px",
                  borderRadius: 16,
                  border: "1px solid",
                  borderColor: active
                    ? "var(--celerex-accent)"
                    : "var(--celerex-border)",
                  background: active ? "var(--celerex-accent)" : "transparent",
                  color: active ? "#fff" : "var(--celerex-text-secondary)",
                  fontSize: 12,
                  fontWeight: 600,
                  cursor: "pointer",
                  transition: "all 0.15s",
                }}
              >
                {et.icon} {et.label} ({count})
              </button>
            );
          })}
        </div>

        {/* Grouped plugin cards */}
        {grouped.length === 0 && (
          <p
            style={{
              textAlign: "center",
              color: "var(--celerex-text-secondary)",
              fontSize: 14,
              marginTop: 48,
            }}
          >
            No plugins match the current filter.
          </p>
        )}

        {grouped.map(({ type, tools }) => {
          const meta = getExecMeta(type)!;
          return (
            <div key={type} style={{ marginBottom: 32 }}>
              <h2
                style={{
                  margin: "0 0 12px",
                  fontSize: 16,
                  fontWeight: 600,
                  color: "var(--celerex-text-primary)",
                  display: "flex",
                  alignItems: "center",
                  gap: 8,
                }}
              >
                <span>{meta.icon}</span> {meta.label}
                <span
                  style={{
                    fontSize: 12,
                    fontWeight: 500,
                    color: "var(--celerex-text-secondary)",
                  }}
                >
                  ({tools.length})
                </span>
              </h2>
              <div
                style={{
                  display: "grid",
                  gridTemplateColumns: "repeat(auto-fill, minmax(340px, 1fr))",
                  gap: 16,
                }}
              >
                {tools.map((plugin) => (
                  <PluginCard
                    key={plugin.id}
                    plugin={plugin}
                    onClick={() => setSelectedPlugin(plugin)}
                  />
                ))}
              </div>
            </div>
          );
        })}
      </div>

      {/* Detail modal */}
      {selectedPlugin && (
        <PluginDetail
          plugin={selectedPlugin}
          onClose={() => setSelectedPlugin(null)}
        />
      )}
    </div>
  );
}
