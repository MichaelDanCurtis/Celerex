import { useState, useMemo } from "react";
import {
  getAllTools,
  searchTools,
  addCustomTool,
  updateCustomTool,
  deleteCustomTool,
  TOOL_CATEGORIES,
  EXECUTION_TYPES,
  SANDBOX_TARGETS,
  type ToolDef,
  type ToolCategoryId,
  type ToolParameter,
  type ExecutionConfig,
  type ExecutionType,
  type SandboxTarget,
} from "../data/tools";
import { getAllAgents } from "../data/agents";

export function ToolsManager() {
  const [search, setSearch] = useState("");
  const [categoryFilter, setCategoryFilter] = useState<ToolCategoryId | "all">("all");
  const [selectedTool, setSelectedTool] = useState<ToolDef | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const [editingTool, setEditingTool] = useState<ToolDef | null>(null);
  const [refreshKey, setRefreshKey] = useState(0);

  const filtered = useMemo(() => {
    let results = search
      ? searchTools(search, categoryFilter === "all" ? undefined : categoryFilter)
      : categoryFilter === "all"
      ? getAllTools()
      : searchTools("", categoryFilter);
    return results;
  }, [search, categoryFilter, refreshKey]);

  const counts = useMemo(() => {
    const all = getAllTools();
    const byCategory: Record<string, number> = {};
    for (const cat of TOOL_CATEGORIES) {
      byCategory[cat.id] = all.filter((t) => t.category === cat.id).length;
    }
    return { total: all.length, byCategory };
  }, [refreshKey]);

  const handleCreate = () => {
    setEditingTool(null);
    setEditorOpen(true);
  };

  const handleEdit = (tool: ToolDef) => {
    setEditingTool(tool);
    setEditorOpen(true);
  };

  const handleSave = (tool: ToolDef) => {
    if (editingTool?.isCustom) {
      updateCustomTool(tool.id, tool);
    } else if (!editingTool) {
      addCustomTool(tool);
    }
    setEditorOpen(false);
    setRefreshKey((k) => k + 1);
  };

  const handleDelete = (id: string) => {
    deleteCustomTool(id);
    setEditorOpen(false);
    setSelectedTool(null);
    setRefreshKey((k) => k + 1);
  };

  const getCategoryColor = (catId: string) =>
    TOOL_CATEGORIES.find((c) => c.id === catId)?.color ?? "#6b7280";

  const getCategoryLabel = (catId: string) =>
    TOOL_CATEGORIES.find((c) => c.id === catId)?.label ?? catId;

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
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
          }}
        >
          <h1
            style={{
              margin: 0,
              fontSize: 22,
              fontWeight: 700,
              color: "var(--celerex-text-primary)",
            }}
          >
            Tools Manager
          </h1>
          <button
            onClick={handleCreate}
            style={{
              padding: "8px 20px",
              borderRadius: 8,
              border: "none",
              background: "var(--celerex-accent)",
              color: "#fff",
              fontSize: 13,
              fontWeight: 600,
              cursor: "pointer",
              display: "inline-flex",
              alignItems: "center",
              gap: 6,
            }}
          >
            + Create Tool
          </button>
        </div>
        <p
          style={{
            margin: "4px 0 20px",
            fontSize: 14,
            color: "var(--celerex-text-secondary)",
          }}
        >
          {counts.total} tools available across {TOOL_CATEGORIES.length} categories — assign tools to agents so they can interact with your systems
        </p>

        {/* Category Pills */}
        <div
          style={{
            display: "flex",
            gap: 6,
            flexWrap: "wrap",
            marginBottom: 16,
          }}
        >
          <button
            onClick={() => setCategoryFilter("all")}
            style={{
              padding: "5px 14px",
              borderRadius: 20,
              border: categoryFilter === "all" ? "none" : "1px solid var(--celerex-border)",
              background: categoryFilter === "all" ? "var(--celerex-accent)" : "transparent",
              color: categoryFilter === "all" ? "#fff" : "var(--celerex-text-secondary)",
              fontSize: 12,
              fontWeight: 500,
              cursor: "pointer",
            }}
          >
            All ({counts.total})
          </button>
          {TOOL_CATEGORIES.map((cat) => {
            const isActive = categoryFilter === cat.id;
            return (
              <button
                key={cat.id}
                onClick={() => setCategoryFilter(isActive ? "all" : cat.id)}
                style={{
                  padding: "5px 14px",
                  borderRadius: 20,
                  border: isActive ? "none" : `1px solid ${cat.color}44`,
                  background: isActive ? cat.color : cat.color + "15",
                  color: isActive ? "#fff" : cat.color,
                  fontSize: 12,
                  fontWeight: 500,
                  cursor: "pointer",
                }}
              >
                {cat.label} ({counts.byCategory[cat.id] ?? 0})
              </button>
            );
          })}
        </div>

        {/* Search */}
        <div style={{ marginBottom: 16 }}>
          <input
            type="text"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search tools..."
            style={{
              width: "100%",
              boxSizing: "border-box",
              padding: "8px 12px",
              borderRadius: 6,
              border: "1px solid var(--celerex-border)",
              background: "var(--celerex-bg-secondary)",
              color: "var(--celerex-text-primary)",
              fontSize: 13,
              outline: "none",
            }}
          />
        </div>

        {/* Selected Tool Detail */}
        {selectedTool && (
          <div
            style={{
              marginBottom: 16,
              padding: 20,
              borderRadius: 10,
              border: `1px solid ${getCategoryColor(selectedTool.category)}`,
              background: "var(--celerex-bg-sidebar)",
            }}
          >
            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                marginBottom: 8,
              }}
            >
              <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                <h3
                  style={{
                    margin: 0,
                    fontSize: 16,
                    fontWeight: 600,
                    color: "var(--celerex-text-primary)",
                  }}
                >
                  {selectedTool.name}
                </h3>
                <span
                  style={{
                    padding: "2px 10px",
                    borderRadius: 10,
                    fontSize: 11,
                    fontWeight: 600,
                    background: getCategoryColor(selectedTool.category) + "22",
                    color: getCategoryColor(selectedTool.category),
                  }}
                >
                  {getCategoryLabel(selectedTool.category)}
                </span>
                {selectedTool.isCustom && (
                  <span
                    style={{
                      fontSize: 11,
                      fontWeight: 500,
                      color: "var(--celerex-accent)",
                    }}
                  >
                    Custom
                  </span>
                )}
              </div>
              <button
                onClick={() => setSelectedTool(null)}
                style={{
                  background: "none",
                  border: "none",
                  fontSize: 18,
                  color: "var(--celerex-text-secondary)",
                  cursor: "pointer",
                }}
              >
                ×
              </button>
            </div>
            <p
              style={{
                margin: "0 0 12px",
                fontSize: 13,
                lineHeight: 1.5,
                color: "var(--celerex-text-secondary)",
              }}
            >
              {selectedTool.description}
            </p>
            <div style={{ fontSize: 11, color: "var(--celerex-text-secondary)", marginBottom: 8 }}>
              ID: <code style={{ fontFamily: "monospace" }}>{selectedTool.id}</code>
            </div>

            {/* Parameters */}
            {selectedTool.parameters.length > 0 && (
              <div style={{ marginBottom: 12 }}>
                <div
                  style={{
                    fontSize: 12,
                    fontWeight: 600,
                    color: "var(--celerex-text-primary)",
                    marginBottom: 6,
                  }}
                >
                  Parameters
                </div>
                <div
                  style={{
                    borderRadius: 6,
                    border: "1px solid var(--celerex-border)",
                    overflow: "hidden",
                  }}
                >
                  <table
                    style={{
                      width: "100%",
                      borderCollapse: "collapse",
                      fontSize: 12,
                    }}
                  >
                    <thead>
                      <tr
                        style={{
                          background: "var(--celerex-bg-primary)",
                        }}
                      >
                        <th style={thStyle}>Name</th>
                        <th style={thStyle}>Type</th>
                        <th style={thStyle}>Required</th>
                        <th style={thStyle}>Description</th>
                      </tr>
                    </thead>
                    <tbody>
                      {selectedTool.parameters.map((param) => (
                        <tr key={param.name}>
                          <td style={tdStyle}>
                            <code style={{ fontFamily: "monospace", fontWeight: 600 }}>
                              {param.name}
                            </code>
                          </td>
                          <td style={tdStyle}>
                            <span
                              style={{
                                padding: "1px 6px",
                                borderRadius: 4,
                                fontSize: 10,
                                fontWeight: 600,
                                background: "var(--celerex-bg-primary)",
                                color: "var(--celerex-text-secondary)",
                              }}
                            >
                              {param.type}
                            </span>
                          </td>
                          <td style={tdStyle}>
                            {param.required ? (
                              <span style={{ color: "#ef4444", fontWeight: 600 }}>Yes</span>
                            ) : (
                              <span style={{ opacity: 0.5 }}>No</span>
                            )}
                          </td>
                          <td style={tdStyle}>
                            {param.description}
                            {param.enum && (
                              <div style={{ marginTop: 2 }}>
                                {param.enum.map((v) => (
                                  <span
                                    key={v}
                                    style={{
                                      display: "inline-block",
                                      padding: "0 5px",
                                      marginRight: 3,
                                      borderRadius: 3,
                                      fontSize: 10,
                                      background: "var(--celerex-bg-primary)",
                                      color: "var(--celerex-text-secondary)",
                                    }}
                                  >
                                    {v}
                                  </span>
                                ))}
                              </div>
                            )}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>
            )}

            {/* Execution Config */}
            <ExecutionDisplay execution={selectedTool.execution} />

            <div style={{ display: "flex", gap: 8 }}>
              {selectedTool.isCustom && (
                <>
                  <button
                    onClick={() => handleEdit(selectedTool)}
                    style={actionBtnStyle}
                  >
                    Edit
                  </button>
                  <button
                    onClick={() => handleDelete(selectedTool.id)}
                    style={{ ...actionBtnStyle, background: "#ef4444", color: "#fff", border: "none" }}
                  >
                    Delete
                  </button>
                </>
              )}
              <button
                onClick={() => {
                  navigator.clipboard.writeText(JSON.stringify(selectedTool, null, 2));
                }}
                style={actionBtnStyle}
              >
                Copy JSON
              </button>
            </div>
          </div>
        )}

        {/* Results count */}
        <div
          style={{
            fontSize: 12,
            color: "var(--celerex-text-secondary)",
            marginBottom: 12,
          }}
        >
          Showing {filtered.length} tools
          {search && ` matching "${search}"`}
          {categoryFilter !== "all" && ` in ${getCategoryLabel(categoryFilter)}`}
        </div>

        {/* Tool Grid */}
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fill, minmax(320px, 1fr))",
            gap: 12,
          }}
        >
          {filtered.map((tool) => {
            const color = getCategoryColor(tool.category);
            const isSelected = selectedTool?.id === tool.id;
            return (
              <div
                key={tool.id}
                onClick={() =>
                  setSelectedTool(isSelected ? null : tool)
                }
                style={{
                  padding: "12px 14px",
                  borderRadius: 8,
                  border: isSelected
                    ? `1px solid ${color}`
                    : "1px solid var(--celerex-border)",
                  background: isSelected
                    ? "var(--celerex-bg-secondary)"
                    : "var(--celerex-bg-sidebar)",
                  cursor: "pointer",
                  transition: "border-color 0.15s, background 0.15s",
                  display: "flex",
                  flexDirection: "column",
                  gap: 6,
                }}
              >
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                  }}
                >
                  <span
                    style={{
                      fontSize: 13,
                      fontWeight: 600,
                      color: "var(--celerex-text-primary)",
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {tool.name}
                  </span>
                  <span
                    style={{
                      padding: "2px 8px",
                      borderRadius: 10,
                      fontSize: 10,
                      fontWeight: 600,
                      background: color + "22",
                      color: color,
                      whiteSpace: "nowrap",
                    }}
                  >
                    {getCategoryLabel(tool.category)}
                  </span>
                </div>
                <p
                  style={{
                    margin: 0,
                    fontSize: 12,
                    lineHeight: 1.4,
                    color: "var(--celerex-text-secondary)",
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    display: "-webkit-box",
                    WebkitLineClamp: 2,
                    WebkitBoxOrient: "vertical",
                  }}
                >
                  {tool.description}
                </p>
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 6,
                    fontSize: 10,
                    color: "var(--celerex-text-secondary)",
                  }}
                >
                  <span>{tool.parameters.length} params</span>
                  <span style={{ opacity: 0.3 }}>|</span>
                  <span>
                    {tool.parameters.filter((p) => p.required).length} required
                  </span>
                  {tool.isCustom && (
                    <>
                      <span style={{ opacity: 0.3 }}>|</span>
                      <span style={{ color: "var(--celerex-accent)", fontWeight: 600 }}>
                        Custom
                      </span>
                    </>
                  )}
                </div>
              </div>
            );
          })}
        </div>

        {filtered.length === 0 && (
          <div
            style={{
              padding: "40px 16px",
              textAlign: "center",
              color: "var(--celerex-text-secondary)",
              fontSize: 14,
            }}
          >
            No tools found{search ? ` matching "${search}"` : ""}
          </div>
        )}
      </div>

      {/* Tool Editor Dialog */}
      {editorOpen && (
        <ToolEditorDialog
          tool={editingTool}
          onSave={handleSave}
          onClose={() => setEditorOpen(false)}
          onDelete={editingTool?.isCustom ? handleDelete : undefined}
        />
      )}
    </div>
  );
}

/* ---- Styles ---- */

const thStyle: React.CSSProperties = {
  textAlign: "left",
  padding: "6px 10px",
  fontSize: 11,
  fontWeight: 600,
  color: "var(--celerex-text-secondary)",
  borderBottom: "1px solid var(--celerex-border)",
};

const tdStyle: React.CSSProperties = {
  padding: "6px 10px",
  fontSize: 12,
  color: "var(--celerex-text-primary)",
  borderBottom: "1px solid var(--celerex-border)",
  verticalAlign: "top",
};

const actionBtnStyle: React.CSSProperties = {
  padding: "6px 16px",
  borderRadius: 6,
  border: "1px solid var(--celerex-border)",
  background: "transparent",
  color: "var(--celerex-text-primary)",
  fontSize: 12,
  fontWeight: 500,
  cursor: "pointer",
};

/* ------------------------------------------------------------------ */
/*  Tool Editor Dialog                                                */
/* ------------------------------------------------------------------ */

function ToolEditorDialog({
  tool,
  onSave,
  onClose,
  onDelete,
}: {
  tool: ToolDef | null;
  onSave: (tool: ToolDef) => void;
  onClose: () => void;
  onDelete?: (id: string) => void;
}) {
  const [name, setName] = useState(tool?.name ?? "");
  const [description, setDescription] = useState(tool?.description ?? "");
  const [category, setCategory] = useState<ToolCategoryId>(tool?.category ?? "code");
  const [parameters, setParameters] = useState<ToolParameter[]>(tool?.parameters ?? []);
  const [execType, setExecType] = useState<ExecutionType | "none">(tool?.execution?.type ?? "none");
  const [execConfig, setExecConfig] = useState<ExecutionConfig | undefined>(tool?.execution);

  const handleAddParam = () => {
    setParameters((prev) => [
      ...prev,
      { name: "", type: "string", description: "", required: false },
    ]);
  };

  const handleRemoveParam = (index: number) => {
    setParameters((prev) => prev.filter((_, i) => i !== index));
  };

  const handleParamChange = (index: number, field: keyof ToolParameter, value: any) => {
    setParameters((prev) =>
      prev.map((p, i) => (i === index ? { ...p, [field]: value } : p))
    );
  };

  const handleSave = () => {
    const id = tool?.id ?? "custom-" + Date.now().toString(36);
    onSave({
      id,
      name,
      description,
      category,
      parameters,
      execution: execType !== "none" ? execConfig : undefined,
      isCustom: true,
    });
  };

  return (
    <div
      onClick={onClose}
      style={{
        position: "fixed",
        inset: 0,
        background: "rgba(0,0,0,0.5)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        zIndex: 1000,
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          width: 600,
          maxHeight: "85vh",
          overflow: "auto",
          background: "var(--celerex-bg-sidebar)",
          borderRadius: 12,
          border: "1px solid var(--celerex-border)",
          padding: 24,
        }}
      >
        <h2
          style={{
            margin: "0 0 16px",
            fontSize: 18,
            fontWeight: 700,
            color: "var(--celerex-text-primary)",
          }}
        >
          {tool ? "Edit Tool" : "Create Tool"}
        </h2>

        {/* Name */}
        <label style={labelStyle}>Name</label>
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="e.g., Send Email"
          style={inputStyle}
        />

        {/* Description */}
        <label style={labelStyle}>Description</label>
        <textarea
          value={description}
          onChange={(e) => setDescription(e.target.value)}
          placeholder="What does this tool do?"
          rows={3}
          style={{ ...inputStyle, resize: "vertical" }}
        />

        {/* Category */}
        <label style={labelStyle}>Category</label>
        <select
          value={category}
          onChange={(e) => setCategory(e.target.value as ToolCategoryId)}
          style={inputStyle}
        >
          {TOOL_CATEGORIES.map((cat) => (
            <option key={cat.id} value={cat.id}>
              {cat.label}
            </option>
          ))}
        </select>

        {/* Parameters */}
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginTop: 16 }}>
          <label style={{ ...labelStyle, margin: 0 }}>Parameters</label>
          <button
            onClick={handleAddParam}
            style={{
              padding: "4px 12px",
              borderRadius: 6,
              border: "1px solid var(--celerex-border)",
              background: "transparent",
              color: "var(--celerex-text-primary)",
              fontSize: 12,
              cursor: "pointer",
            }}
          >
            + Add Parameter
          </button>
        </div>

        {parameters.map((param, i) => (
          <div
            key={i}
            style={{
              marginTop: 8,
              padding: 12,
              borderRadius: 6,
              border: "1px solid var(--celerex-border)",
              background: "var(--celerex-bg-primary)",
            }}
          >
            <div style={{ display: "flex", gap: 8, marginBottom: 6 }}>
              <input
                value={param.name}
                onChange={(e) => handleParamChange(i, "name", e.target.value)}
                placeholder="param_name"
                style={{ ...inputStyle, flex: 1, marginBottom: 0 }}
              />
              <select
                value={param.type}
                onChange={(e) => handleParamChange(i, "type", e.target.value)}
                style={{ ...inputStyle, width: 100, marginBottom: 0 }}
              >
                <option value="string">string</option>
                <option value="number">number</option>
                <option value="boolean">boolean</option>
                <option value="object">object</option>
                <option value="array">array</option>
              </select>
              <label
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 4,
                  fontSize: 11,
                  color: "var(--celerex-text-secondary)",
                  whiteSpace: "nowrap",
                }}
              >
                <input
                  type="checkbox"
                  checked={!!param.required}
                  onChange={(e) => handleParamChange(i, "required", e.target.checked)}
                />
                Required
              </label>
              <button
                onClick={() => handleRemoveParam(i)}
                style={{
                  background: "none",
                  border: "none",
                  color: "#ef4444",
                  fontSize: 16,
                  cursor: "pointer",
                  padding: "0 4px",
                }}
              >
                ×
              </button>
            </div>
            <input
              value={param.description}
              onChange={(e) => handleParamChange(i, "description", e.target.value)}
              placeholder="Description of this parameter"
              style={{ ...inputStyle, marginBottom: 0 }}
            />
          </div>
        ))}

        {/* Execution Backend */}
        <div style={{ marginTop: 20, marginBottom: 4 }}>
          <label style={{ ...labelStyle, margin: 0, marginBottom: 8, fontSize: 14, fontWeight: 700, color: "var(--celerex-text-primary)" }}>
            Execution Backend
          </label>
          <p style={{ margin: "0 0 10px", fontSize: 12, color: "var(--celerex-text-secondary)" }}>
            How should this tool run? Choose an execution type to connect it to a real backend.
          </p>

          {/* Type selector cards */}
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 6, marginBottom: 12 }}>
            <button
              onClick={() => { setExecType("none"); setExecConfig(undefined); }}
              style={{
                padding: "8px 10px",
                borderRadius: 8,
                border: execType === "none" ? "2px solid var(--celerex-accent)" : "1px solid var(--celerex-border)",
                background: execType === "none" ? "var(--celerex-accent)" + "15" : "var(--celerex-bg-primary)",
                color: "var(--celerex-text-primary)",
                fontSize: 12,
                fontWeight: 500,
                cursor: "pointer",
                textAlign: "left",
              }}
            >
              <div style={{ fontWeight: 600 }}>None (metadata only)</div>
            </button>
            {EXECUTION_TYPES.map((et) => (
              <button
                key={et.id}
                onClick={() => {
                  setExecType(et.id);
                  if (!execConfig || execConfig.type !== et.id) {
                    const base: ExecutionConfig = { type: et.id };
                    if (et.id === "mcp") base.mcp = { serverUrl: "", transport: "stdio", command: "", toolName: "" };
                    if (et.id === "http") base.http = { url: "", method: "POST", authType: "none" };
                    if (et.id === "agent") base.agent = { agentId: "" };
                    if (et.id === "script") base.script = { command: "" };
                    if (et.id === "sandbox") base.sandbox = { target: "local_docker", local_docker: { image: "", command: "" } };
                    setExecConfig(base);
                  }
                }}
                style={{
                  padding: "8px 10px",
                  borderRadius: 8,
                  border: execType === et.id ? "2px solid var(--celerex-accent)" : "1px solid var(--celerex-border)",
                  background: execType === et.id ? "var(--celerex-accent)" + "15" : "var(--celerex-bg-primary)",
                  color: "var(--celerex-text-primary)",
                  fontSize: 12,
                  fontWeight: 500,
                  cursor: "pointer",
                  textAlign: "left",
                }}
              >
                <div style={{ fontWeight: 600 }}>{et.icon} {et.label}</div>
                <div style={{ fontSize: 10, color: "var(--celerex-text-secondary)", marginTop: 2 }}>{et.description}</div>
              </button>
            ))}
          </div>

          {/* Type-specific config forms */}
          {execType === "mcp" && execConfig?.mcp && (
            <McpConfigEditor config={execConfig.mcp} onChange={(mcp) => setExecConfig({ ...execConfig, mcp })} />
          )}
          {execType === "http" && execConfig?.http && (
            <HttpConfigEditor config={execConfig.http} onChange={(http) => setExecConfig({ ...execConfig, http })} />
          )}
          {execType === "agent" && execConfig?.agent && (
            <AgentConfigEditor config={execConfig.agent} onChange={(agent) => setExecConfig({ ...execConfig, agent })} />
          )}
          {execType === "script" && execConfig?.script && (
            <ScriptConfigEditor config={execConfig.script} onChange={(script) => setExecConfig({ ...execConfig, script })} />
          )}
          {execType === "sandbox" && execConfig?.sandbox && (
            <SandboxConfigEditor config={execConfig.sandbox} onChange={(sandbox) => setExecConfig({ ...execConfig, sandbox })} />
          )}
        </div>

        {/* Actions */}
        <div style={{ display: "flex", gap: 8, marginTop: 20, justifyContent: "flex-end" }}>
          {onDelete && tool && (
            <button
              onClick={() => onDelete(tool.id)}
              style={{
                padding: "8px 16px",
                borderRadius: 6,
                border: "none",
                background: "#ef4444",
                color: "#fff",
                fontSize: 13,
                fontWeight: 600,
                cursor: "pointer",
                marginRight: "auto",
              }}
            >
              Delete
            </button>
          )}
          <button
            onClick={onClose}
            style={{
              padding: "8px 16px",
              borderRadius: 6,
              border: "1px solid var(--celerex-border)",
              background: "transparent",
              color: "var(--celerex-text-primary)",
              fontSize: 13,
              cursor: "pointer",
            }}
          >
            Cancel
          </button>
          <button
            onClick={handleSave}
            disabled={!name.trim()}
            style={{
              padding: "8px 20px",
              borderRadius: 6,
              border: "none",
              background: "var(--celerex-accent)",
              color: "#fff",
              fontSize: 13,
              fontWeight: 600,
              cursor: name.trim() ? "pointer" : "not-allowed",
              opacity: name.trim() ? 1 : 0.5,
            }}
          >
            Save
          </button>
        </div>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Execution Config Display (read-only, in detail panel)             */
/* ------------------------------------------------------------------ */

function ExecutionDisplay({ execution }: { execution?: ExecutionConfig }) {
  if (!execution) {
    return (
      <div style={{ marginBottom: 12, padding: 12, borderRadius: 8, border: "1px dashed var(--celerex-border)", background: "var(--celerex-bg-primary)" }}>
        <div style={{ fontSize: 12, color: "var(--celerex-text-secondary)", display: "flex", alignItems: "center", gap: 6 }}>
          <span style={{ opacity: 0.5 }}>{"⚡"}</span>
          No execution backend configured — this tool is metadata only
        </div>
      </div>
    );
  }

  const typeInfo = EXECUTION_TYPES.find((t) => t.id === execution.type);

  return (
    <div style={{ marginBottom: 12 }}>
      <div style={{ fontSize: 12, fontWeight: 600, color: "var(--celerex-text-primary)", marginBottom: 6 }}>
        Execution Backend
      </div>
      <div style={{ padding: 12, borderRadius: 8, border: "1px solid var(--celerex-border)", background: "var(--celerex-bg-primary)" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 8 }}>
          <span style={{ fontSize: 16 }}>{typeInfo?.icon ?? "⚡"}</span>
          <span style={{ fontSize: 13, fontWeight: 600, color: "var(--celerex-text-primary)" }}>{typeInfo?.label ?? execution.type}</span>
          <span style={{ fontSize: 11, color: "var(--celerex-text-secondary)" }}>{typeInfo?.description}</span>
        </div>

        {execution.type === "mcp" && execution.mcp && (
          <div style={{ display: "grid", gap: 4, fontSize: 12 }}>
            <KV label="Transport" value={execution.mcp.transport} />
            {execution.mcp.serverUrl && <KV label="Server URL" value={execution.mcp.serverUrl} />}
            {execution.mcp.command && <KV label="Command" value={`${execution.mcp.command} ${(execution.mcp.args ?? []).join(" ")}`} />}
            {execution.mcp.toolName && <KV label="MCP Tool Name" value={execution.mcp.toolName} />}
            {execution.mcp.env && Object.keys(execution.mcp.env).length > 0 && (
              <KV label="Env Vars" value={Object.keys(execution.mcp.env).join(", ")} />
            )}
          </div>
        )}

        {execution.type === "http" && execution.http && (
          <div style={{ display: "grid", gap: 4, fontSize: 12 }}>
            <KV label="Method" value={execution.http.method} />
            <KV label="URL" value={execution.http.url} />
            {execution.http.authType && execution.http.authType !== "none" && (
              <KV label="Auth" value={execution.http.authType} />
            )}
            {execution.http.bodyTemplate && <KV label="Body Template" value={execution.http.bodyTemplate} mono />}
          </div>
        )}

        {execution.type === "agent" && execution.agent && (
          <div style={{ display: "grid", gap: 4, fontSize: 12 }}>
            <KV label="Target Agent" value={execution.agent.agentId} />
            {execution.agent.inputMapping && Object.keys(execution.agent.inputMapping).length > 0 && (
              <KV label="Input Mapping" value={JSON.stringify(execution.agent.inputMapping)} mono />
            )}
          </div>
        )}

        {execution.type === "script" && execution.script && (
          <div style={{ display: "grid", gap: 4, fontSize: 12 }}>
            <KV label="Command" value={`${execution.script.command} ${(execution.script.args ?? []).join(" ")}`} />
            {execution.script.cwd && <KV label="Working Dir" value={execution.script.cwd} />}
            {execution.script.timeout && <KV label="Timeout" value={`${execution.script.timeout}s`} />}
          </div>
        )}

        {execution.type === "sandbox" && execution.sandbox && (
          <SandboxDisplay sandbox={execution.sandbox} />
        )}
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Execution Config Editors                                          */
/* ------------------------------------------------------------------ */

function ConfigSection({ children }: { children: React.ReactNode }) {
  return (
    <div style={{ padding: 12, borderRadius: 8, border: "1px solid var(--celerex-border)", background: "var(--celerex-bg-primary)", display: "grid", gap: 8 }}>
      {children}
    </div>
  );
}

function ConfigField({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <label style={{ display: "block", fontSize: 11, fontWeight: 600, color: "var(--celerex-text-secondary)", marginBottom: 3 }}>{label}</label>
      {children}
    </div>
  );
}

const cfgInputStyle: React.CSSProperties = {
  display: "block",
  width: "100%",
  boxSizing: "border-box",
  padding: "6px 8px",
  borderRadius: 5,
  border: "1px solid var(--celerex-border)",
  background: "var(--celerex-bg-secondary)",
  color: "var(--celerex-text-primary)",
  fontSize: 12,
  fontFamily: "monospace",
  outline: "none",
};

function McpConfigEditor({ config, onChange }: { config: NonNullable<import("../data/tools").McpConfig>; onChange: (c: typeof config) => void }) {
  return (
    <ConfigSection>
      <ConfigField label="Transport">
        <select value={config.transport} onChange={(e) => onChange({ ...config, transport: e.target.value as any })} style={cfgInputStyle}>
          <option value="stdio">stdio (local process)</option>
          <option value="sse">SSE (Server-Sent Events)</option>
          <option value="streamable-http">Streamable HTTP</option>
        </select>
      </ConfigField>

      {config.transport === "stdio" ? (
        <>
          <ConfigField label="Command">
            <input value={config.command ?? ""} onChange={(e) => onChange({ ...config, command: e.target.value })} placeholder="e.g., npx -y @anthropic/mcp-news-server" style={cfgInputStyle} />
          </ConfigField>
          <ConfigField label="Arguments (one per line)">
            <textarea value={(config.args ?? []).join("\n")} onChange={(e) => onChange({ ...config, args: e.target.value.split("\n").filter(Boolean) })} placeholder="--port 3001" rows={2} style={{ ...cfgInputStyle, resize: "vertical" }} />
          </ConfigField>
        </>
      ) : (
        <ConfigField label="Server URL">
          <input value={config.serverUrl} onChange={(e) => onChange({ ...config, serverUrl: e.target.value })} placeholder="e.g., http://localhost:3001/mcp" style={cfgInputStyle} />
        </ConfigField>
      )}

      <ConfigField label="MCP Tool Name (the tool name on the server)">
        <input value={config.toolName ?? ""} onChange={(e) => onChange({ ...config, toolName: e.target.value })} placeholder="e.g., fetch_news" style={cfgInputStyle} />
      </ConfigField>

      <ConfigField label="Environment Variables (KEY=VALUE, one per line)">
        <textarea
          value={Object.entries(config.env ?? {}).map(([k, v]) => `${k}=${v}`).join("\n")}
          onChange={(e) => {
            const env: Record<string, string> = {};
            e.target.value.split("\n").filter(Boolean).forEach((line) => {
              const eq = line.indexOf("=");
              if (eq > 0) env[line.slice(0, eq)] = line.slice(eq + 1);
            });
            onChange({ ...config, env });
          }}
          placeholder="API_KEY=sk-..."
          rows={2}
          style={{ ...cfgInputStyle, resize: "vertical" }}
        />
      </ConfigField>
    </ConfigSection>
  );
}

function HttpConfigEditor({ config, onChange }: { config: NonNullable<import("../data/tools").HttpConfig>; onChange: (c: typeof config) => void }) {
  return (
    <ConfigSection>
      <div style={{ display: "flex", gap: 8 }}>
        <ConfigField label="Method">
          <select value={config.method} onChange={(e) => onChange({ ...config, method: e.target.value as any })} style={{ ...cfgInputStyle, width: 100 }}>
            {["GET", "POST", "PUT", "PATCH", "DELETE"].map((m) => <option key={m} value={m}>{m}</option>)}
          </select>
        </ConfigField>
        <div style={{ flex: 1 }}>
          <ConfigField label="URL">
            <input value={config.url} onChange={(e) => onChange({ ...config, url: e.target.value })} placeholder="https://api.example.com/v1/news" style={cfgInputStyle} />
          </ConfigField>
        </div>
      </div>

      <ConfigField label="Authentication">
        <div style={{ display: "flex", gap: 8 }}>
          <select value={config.authType ?? "none"} onChange={(e) => onChange({ ...config, authType: e.target.value as any })} style={{ ...cfgInputStyle, width: 130 }}>
            <option value="none">None</option>
            <option value="bearer">Bearer Token</option>
            <option value="api-key">API Key</option>
            <option value="basic">Basic Auth</option>
          </select>
          {config.authType && config.authType !== "none" && (
            <input
              type="password"
              value={config.authValue ?? ""}
              onChange={(e) => onChange({ ...config, authValue: e.target.value })}
              placeholder={config.authType === "bearer" ? "Token..." : config.authType === "api-key" ? "API key..." : "user:pass"}
              style={{ ...cfgInputStyle, flex: 1 }}
            />
          )}
        </div>
        {config.authType === "api-key" && (
          <input value={config.authHeader ?? ""} onChange={(e) => onChange({ ...config, authHeader: e.target.value })} placeholder="Header name (default: X-API-Key)" style={{ ...cfgInputStyle, marginTop: 4 }} />
        )}
      </ConfigField>

      <ConfigField label="Headers (KEY: VALUE, one per line)">
        <textarea
          value={Object.entries(config.headers ?? {}).map(([k, v]) => `${k}: ${v}`).join("\n")}
          onChange={(e) => {
            const headers: Record<string, string> = {};
            e.target.value.split("\n").filter(Boolean).forEach((line) => {
              const colon = line.indexOf(":");
              if (colon > 0) headers[line.slice(0, colon).trim()] = line.slice(colon + 1).trim();
            });
            onChange({ ...config, headers });
          }}
          placeholder="Content-Type: application/json"
          rows={2}
          style={{ ...cfgInputStyle, resize: "vertical" }}
        />
      </ConfigField>

      <ConfigField label="Body Template (JSON — use {{paramName}} for interpolation)">
        <textarea
          value={config.bodyTemplate ?? ""}
          onChange={(e) => onChange({ ...config, bodyTemplate: e.target.value })}
          placeholder={'{\n  "query": "{{query}}",\n  "limit": {{limit}}\n}'}
          rows={4}
          style={{ ...cfgInputStyle, resize: "vertical" }}
        />
      </ConfigField>
    </ConfigSection>
  );
}

function AgentConfigEditor({ config, onChange }: { config: NonNullable<import("../data/tools").AgentConfig>; onChange: (c: typeof config) => void }) {
  const agents = getAllAgents();
  const [agentSearch, setAgentSearch] = useState("");
  const filteredAgents = agentSearch
    ? agents.filter((a) => a.name.toLowerCase().includes(agentSearch.toLowerCase()) || a.id.includes(agentSearch.toLowerCase())).slice(0, 20)
    : [];

  const selectedAgent = agents.find((a) => a.id === config.agentId);

  return (
    <ConfigSection>
      <ConfigField label="Target Agent">
        {selectedAgent ? (
          <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "6px 8px", borderRadius: 5, background: "var(--celerex-bg-secondary)", border: "1px solid var(--celerex-border)" }}>
            <span style={{ fontSize: 12, fontWeight: 600, color: "var(--celerex-text-primary)" }}>{selectedAgent.name}</span>
            <span style={{ fontSize: 10, color: "var(--celerex-text-secondary)" }}>{selectedAgent.id}</span>
            <button onClick={() => onChange({ ...config, agentId: "" })} style={{ marginLeft: "auto", background: "none", border: "none", color: "#ef4444", cursor: "pointer", fontSize: 14 }}>{"×"}</button>
          </div>
        ) : (
          <div>
            <input value={agentSearch} onChange={(e) => setAgentSearch(e.target.value)} placeholder="Search agents..." style={cfgInputStyle} />
            {filteredAgents.length > 0 && (
              <div style={{ maxHeight: 150, overflow: "auto", border: "1px solid var(--celerex-border)", borderRadius: 5, marginTop: 4 }}>
                {filteredAgents.map((a) => (
                  <div key={a.id} onClick={() => { onChange({ ...config, agentId: a.id }); setAgentSearch(""); }} style={{ padding: "6px 8px", cursor: "pointer", fontSize: 12, borderBottom: "1px solid var(--celerex-border)" }}
                    onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = "var(--celerex-bg-secondary)"; }}
                    onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "transparent"; }}
                  >
                    <div style={{ fontWeight: 600, color: "var(--celerex-text-primary)" }}>{a.name}</div>
                    <div style={{ fontSize: 10, color: "var(--celerex-text-secondary)" }}>{a.category} · {a.id}</div>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}
      </ConfigField>

      <ConfigField label="Input Mapping (tool param → agent input, JSON)">
        <textarea
          value={config.inputMapping ? JSON.stringify(config.inputMapping, null, 2) : ""}
          onChange={(e) => {
            try {
              const mapping = e.target.value.trim() ? JSON.parse(e.target.value) : undefined;
              onChange({ ...config, inputMapping: mapping });
            } catch { /* ignore parse errors while typing */ }
          }}
          placeholder={'{\n  "query": "input_text"\n}'}
          rows={3}
          style={{ ...cfgInputStyle, resize: "vertical" }}
        />
      </ConfigField>
    </ConfigSection>
  );
}

function ScriptConfigEditor({ config, onChange }: { config: NonNullable<import("../data/tools").ScriptConfig>; onChange: (c: typeof config) => void }) {
  return (
    <ConfigSection>
      <ConfigField label="Command">
        <input value={config.command} onChange={(e) => onChange({ ...config, command: e.target.value })} placeholder="e.g., python3 scripts/fetch_news.py" style={cfgInputStyle} />
      </ConfigField>

      <ConfigField label="Arguments (one per line)">
        <textarea value={(config.args ?? []).join("\n")} onChange={(e) => onChange({ ...config, args: e.target.value.split("\n").filter(Boolean) })} placeholder="--format json" rows={2} style={{ ...cfgInputStyle, resize: "vertical" }} />
      </ConfigField>

      <ConfigField label="Working Directory">
        <input value={config.cwd ?? ""} onChange={(e) => onChange({ ...config, cwd: e.target.value || undefined })} placeholder="/path/to/project" style={cfgInputStyle} />
      </ConfigField>

      <ConfigField label="Timeout (seconds)">
        <input type="number" value={config.timeout ?? ""} onChange={(e) => onChange({ ...config, timeout: e.target.value ? Number(e.target.value) : undefined })} placeholder="30" style={{ ...cfgInputStyle, width: 120 }} />
      </ConfigField>

      <ConfigField label="Environment Variables (KEY=VALUE, one per line)">
        <textarea
          value={Object.entries(config.env ?? {}).map(([k, v]) => `${k}=${v}`).join("\n")}
          onChange={(e) => {
            const env: Record<string, string> = {};
            e.target.value.split("\n").filter(Boolean).forEach((line) => {
              const eq = line.indexOf("=");
              if (eq > 0) env[line.slice(0, eq)] = line.slice(eq + 1);
            });
            onChange({ ...config, env: Object.keys(env).length > 0 ? env : undefined });
          }}
          placeholder="API_KEY=sk-..."
          rows={2}
          style={{ ...cfgInputStyle, resize: "vertical" }}
        />
      </ConfigField>
    </ConfigSection>
  );
}

function SandboxConfigEditor({ config, onChange }: { config: NonNullable<import("../data/tools").SandboxConfig>; onChange: (c: typeof config) => void }) {
  const handleTargetChange = (target: SandboxTarget) => {
    const base: import("../data/tools").SandboxConfig = { target };
    if (target === "local_docker") base.local_docker = { image: "", command: "" };
    if (target === "docker_host") base.docker_host = { host_url: "", image: "", command: "" };
    if (target === "slurm") base.slurm = { cluster_url: "", partition: "", command: "" };
    if (target === "cloud_function") base.cloud_function = { provider: "gcp", function_name: "", region: "" };
    if (target === "kubernetes") base.kubernetes = { cluster: "", namespace: "default", image: "", command: "" };
    onChange(base);
  };

  return (
    <ConfigSection>
      <ConfigField label="Execution Target">
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr 1fr", gap: 4 }}>
          {SANDBOX_TARGETS.map((t) => (
            <button key={t.id} onClick={() => handleTargetChange(t.id)} style={{
              padding: "6px 8px", borderRadius: 6, fontSize: 11, cursor: "pointer", textAlign: "left",
              border: config.target === t.id ? "2px solid var(--celerex-accent)" : "1px solid var(--celerex-border)",
              background: config.target === t.id ? "var(--celerex-accent)" + "15" : "transparent",
              color: "var(--celerex-text-primary)",
            }}>
              <div style={{ fontWeight: 600 }}>{t.icon} {t.label}</div>
              <div style={{ fontSize: 9, color: "var(--celerex-text-secondary)", marginTop: 1 }}>{t.description}</div>
            </button>
          ))}
        </div>
      </ConfigField>

      {config.target === "local_docker" && config.local_docker && (
        <>
          <ConfigField label="Docker Image">
            <input value={config.local_docker.image} onChange={(e) => onChange({ ...config, local_docker: { ...config.local_docker!, image: e.target.value } })} placeholder="python:3.12-slim" style={cfgInputStyle} />
          </ConfigField>
          <ConfigField label="Command">
            <input value={config.local_docker.command} onChange={(e) => onChange({ ...config, local_docker: { ...config.local_docker!, command: e.target.value } })} placeholder="python3 -c" style={cfgInputStyle} />
          </ConfigField>
          <ConfigField label="Timeout (seconds)">
            <input type="number" value={config.local_docker.timeout ?? ""} onChange={(e) => onChange({ ...config, local_docker: { ...config.local_docker!, timeout: e.target.value ? Number(e.target.value) : undefined } })} placeholder="60" style={{ ...cfgInputStyle, width: 120 }} />
          </ConfigField>
        </>
      )}

      {config.target === "docker_host" && config.docker_host && (
        <>
          <ConfigField label="Remote Docker Host URL">
            <input value={config.docker_host.host_url} onChange={(e) => onChange({ ...config, docker_host: { ...config.docker_host!, host_url: e.target.value } })} placeholder="tcp://vm-host:2376 or ssh://user@host" style={cfgInputStyle} />
          </ConfigField>
          <ConfigField label="Docker Image">
            <input value={config.docker_host.image} onChange={(e) => onChange({ ...config, docker_host: { ...config.docker_host!, image: e.target.value } })} placeholder="python:3.12-slim" style={cfgInputStyle} />
          </ConfigField>
          <ConfigField label="Command">
            <input value={config.docker_host.command} onChange={(e) => onChange({ ...config, docker_host: { ...config.docker_host!, command: e.target.value } })} placeholder="python3 -c" style={cfgInputStyle} />
          </ConfigField>
          <div style={{ display: "flex", gap: 8 }}>
            <ConfigField label="Memory Limit">
              <input value={config.docker_host.memory_limit ?? ""} onChange={(e) => onChange({ ...config, docker_host: { ...config.docker_host!, memory_limit: e.target.value || undefined } })} placeholder="4g" style={{ ...cfgInputStyle, width: 100 }} />
            </ConfigField>
            <ConfigField label="CPU Limit">
              <input type="number" value={config.docker_host.cpu_limit ?? ""} onChange={(e) => onChange({ ...config, docker_host: { ...config.docker_host!, cpu_limit: e.target.value ? Number(e.target.value) : undefined } })} placeholder="4" style={{ ...cfgInputStyle, width: 80 }} />
            </ConfigField>
            <ConfigField label="GPU">
              <label style={{ display: "flex", alignItems: "center", gap: 4, fontSize: 11, color: "var(--celerex-text-secondary)" }}>
                <input type="checkbox" checked={!!config.docker_host.gpu} onChange={(e) => onChange({ ...config, docker_host: { ...config.docker_host!, gpu: e.target.checked } })} /> Enable GPU
              </label>
            </ConfigField>
          </div>
        </>
      )}

      {config.target === "slurm" && config.slurm && (
        <>
          <ConfigField label="Cluster SSH Endpoint">
            <input value={config.slurm.cluster_url} onChange={(e) => onChange({ ...config, slurm: { ...config.slurm!, cluster_url: e.target.value } })} placeholder="user@slurm-head.cluster.local" style={cfgInputStyle} />
          </ConfigField>
          <div style={{ display: "flex", gap: 8 }}>
            <div style={{ flex: 1 }}><ConfigField label="Partition">
              <input value={config.slurm.partition} onChange={(e) => onChange({ ...config, slurm: { ...config.slurm!, partition: e.target.value } })} placeholder="gpu" style={cfgInputStyle} />
            </ConfigField></div>
            <div style={{ flex: 1 }}><ConfigField label="Time Limit">
              <input value={config.slurm.time_limit ?? ""} onChange={(e) => onChange({ ...config, slurm: { ...config.slurm!, time_limit: e.target.value || undefined } })} placeholder="04:00:00" style={cfgInputStyle} />
            </ConfigField></div>
          </div>
          <ConfigField label="Command">
            <input value={config.slurm.command} onChange={(e) => onChange({ ...config, slurm: { ...config.slurm!, command: e.target.value } })} placeholder="python train.py" style={cfgInputStyle} />
          </ConfigField>
          <div style={{ display: "flex", gap: 8 }}>
            <ConfigField label="Memory">
              <input value={config.slurm.memory ?? ""} onChange={(e) => onChange({ ...config, slurm: { ...config.slurm!, memory: e.target.value || undefined } })} placeholder="32G" style={{ ...cfgInputStyle, width: 80 }} />
            </ConfigField>
            <ConfigField label="CPUs">
              <input type="number" value={config.slurm.cpus_per_task ?? ""} onChange={(e) => onChange({ ...config, slurm: { ...config.slurm!, cpus_per_task: e.target.value ? Number(e.target.value) : undefined } })} placeholder="8" style={{ ...cfgInputStyle, width: 70 }} />
            </ConfigField>
            <ConfigField label="GPUs">
              <input value={config.slurm.gpus ?? ""} onChange={(e) => onChange({ ...config, slurm: { ...config.slurm!, gpus: e.target.value || undefined } })} placeholder="a100:1" style={{ ...cfgInputStyle, width: 100 }} />
            </ConfigField>
          </div>
          <ConfigField label="Modules to load (one per line)">
            <textarea value={(config.slurm.modules ?? []).join("\n")} onChange={(e) => onChange({ ...config, slurm: { ...config.slurm!, modules: e.target.value.split("\n").filter(Boolean) } })} placeholder="cuda/12.1&#10;python/3.12" rows={2} style={{ ...cfgInputStyle, resize: "vertical" }} />
          </ConfigField>
        </>
      )}

      {config.target === "cloud_function" && config.cloud_function && (
        <>
          <ConfigField label="Cloud Provider">
            <select value={config.cloud_function.provider} onChange={(e) => onChange({ ...config, cloud_function: { ...config.cloud_function!, provider: e.target.value as any } })} style={cfgInputStyle}>
              <option value="gcp">Google Cloud Functions</option>
              <option value="aws_lambda">AWS Lambda</option>
              <option value="azure_functions">Azure Functions</option>
            </select>
          </ConfigField>
          <ConfigField label="Function Name / ARN">
            <input value={config.cloud_function.function_name} onChange={(e) => onChange({ ...config, cloud_function: { ...config.cloud_function!, function_name: e.target.value } })} placeholder="my-function or arn:aws:lambda:..." style={cfgInputStyle} />
          </ConfigField>
          <div style={{ display: "flex", gap: 8 }}>
            <div style={{ flex: 1 }}><ConfigField label="Region">
              <input value={config.cloud_function.region} onChange={(e) => onChange({ ...config, cloud_function: { ...config.cloud_function!, region: e.target.value } })} placeholder="us-central1" style={cfgInputStyle} />
            </ConfigField></div>
            <div style={{ flex: 1 }}><ConfigField label="Runtime">
              <input value={config.cloud_function.runtime ?? ""} onChange={(e) => onChange({ ...config, cloud_function: { ...config.cloud_function!, runtime: e.target.value || undefined } })} placeholder="python3.12" style={cfgInputStyle} />
            </ConfigField></div>
          </div>
          <div style={{ display: "flex", gap: 8 }}>
            <ConfigField label="Memory (MB)">
              <input type="number" value={config.cloud_function.memory ?? ""} onChange={(e) => onChange({ ...config, cloud_function: { ...config.cloud_function!, memory: e.target.value ? Number(e.target.value) : undefined } })} placeholder="512" style={{ ...cfgInputStyle, width: 100 }} />
            </ConfigField>
            <ConfigField label="Timeout (seconds)">
              <input type="number" value={config.cloud_function.timeout ?? ""} onChange={(e) => onChange({ ...config, cloud_function: { ...config.cloud_function!, timeout: e.target.value ? Number(e.target.value) : undefined } })} placeholder="60" style={{ ...cfgInputStyle, width: 100 }} />
            </ConfigField>
          </div>
        </>
      )}

      {config.target === "kubernetes" && config.kubernetes && (
        <>
          <div style={{ display: "flex", gap: 8 }}>
            <div style={{ flex: 1 }}><ConfigField label="Cluster">
              <input value={config.kubernetes.cluster} onChange={(e) => onChange({ ...config, kubernetes: { ...config.kubernetes!, cluster: e.target.value } })} placeholder="production-cluster" style={cfgInputStyle} />
            </ConfigField></div>
            <div style={{ flex: 1 }}><ConfigField label="Namespace">
              <input value={config.kubernetes.namespace} onChange={(e) => onChange({ ...config, kubernetes: { ...config.kubernetes!, namespace: e.target.value } })} placeholder="default" style={cfgInputStyle} />
            </ConfigField></div>
          </div>
          <ConfigField label="Container Image">
            <input value={config.kubernetes.image} onChange={(e) => onChange({ ...config, kubernetes: { ...config.kubernetes!, image: e.target.value } })} placeholder="myregistry/my-app:latest" style={cfgInputStyle} />
          </ConfigField>
          <ConfigField label="Command">
            <input value={config.kubernetes.command} onChange={(e) => onChange({ ...config, kubernetes: { ...config.kubernetes!, command: e.target.value } })} placeholder="python -m pipeline.run" style={cfgInputStyle} />
          </ConfigField>
          <div style={{ display: "flex", gap: 8 }}>
            <ConfigField label="CPU Request">
              <input value={config.kubernetes.cpu_request ?? ""} onChange={(e) => onChange({ ...config, kubernetes: { ...config.kubernetes!, cpu_request: e.target.value || undefined } })} placeholder="500m" style={{ ...cfgInputStyle, width: 80 }} />
            </ConfigField>
            <ConfigField label="CPU Limit">
              <input value={config.kubernetes.cpu_limit ?? ""} onChange={(e) => onChange({ ...config, kubernetes: { ...config.kubernetes!, cpu_limit: e.target.value || undefined } })} placeholder="2000m" style={{ ...cfgInputStyle, width: 80 }} />
            </ConfigField>
            <ConfigField label="Memory Request">
              <input value={config.kubernetes.memory_request ?? ""} onChange={(e) => onChange({ ...config, kubernetes: { ...config.kubernetes!, memory_request: e.target.value || undefined } })} placeholder="1Gi" style={{ ...cfgInputStyle, width: 80 }} />
            </ConfigField>
            <ConfigField label="Memory Limit">
              <input value={config.kubernetes.memory_limit ?? ""} onChange={(e) => onChange({ ...config, kubernetes: { ...config.kubernetes!, memory_limit: e.target.value || undefined } })} placeholder="4Gi" style={{ ...cfgInputStyle, width: 80 }} />
            </ConfigField>
          </div>
          <ConfigField label="Service Account">
            <input value={config.kubernetes.service_account ?? ""} onChange={(e) => onChange({ ...config, kubernetes: { ...config.kubernetes!, service_account: e.target.value || undefined } })} placeholder="my-service-account" style={cfgInputStyle} />
          </ConfigField>
          <ConfigField label="Image Pull Secret">
            <input value={config.kubernetes.image_pull_secret ?? ""} onChange={(e) => onChange({ ...config, kubernetes: { ...config.kubernetes!, image_pull_secret: e.target.value || undefined } })} placeholder="registry-credentials" style={cfgInputStyle} />
          </ConfigField>
        </>
      )}
    </ConfigSection>
  );
}

function SandboxDisplay({ sandbox }: { sandbox: import("../data/tools").SandboxConfig }) {
  const targetInfo = SANDBOX_TARGETS.find((t) => t.id === sandbox.target);
  return (
    <div style={{ display: "grid", gap: 4, fontSize: 12 }}>
      <KV label="Target" value={`${targetInfo?.icon ?? ""} ${targetInfo?.label ?? sandbox.target}`} />

      {sandbox.target === "local_docker" && sandbox.local_docker && (
        <>
          <KV label="Image" value={sandbox.local_docker.image} />
          <KV label="Command" value={sandbox.local_docker.command} />
          {sandbox.local_docker.timeout && <KV label="Timeout" value={`${sandbox.local_docker.timeout}s`} />}
        </>
      )}

      {sandbox.target === "docker_host" && sandbox.docker_host && (
        <>
          <KV label="Host" value={sandbox.docker_host.host_url} />
          <KV label="Image" value={sandbox.docker_host.image} />
          <KV label="Command" value={sandbox.docker_host.command} />
          {sandbox.docker_host.memory_limit && <KV label="Memory" value={sandbox.docker_host.memory_limit} />}
          {sandbox.docker_host.cpu_limit && <KV label="CPUs" value={String(sandbox.docker_host.cpu_limit)} />}
          {sandbox.docker_host.gpu && <KV label="GPU" value="Enabled" />}
        </>
      )}

      {sandbox.target === "slurm" && sandbox.slurm && (
        <>
          <KV label="Cluster" value={sandbox.slurm.cluster_url} />
          <KV label="Partition" value={sandbox.slurm.partition} />
          <KV label="Command" value={sandbox.slurm.command} />
          {sandbox.slurm.gpus && <KV label="GPUs" value={sandbox.slurm.gpus} />}
          {sandbox.slurm.memory && <KV label="Memory" value={sandbox.slurm.memory} />}
          {sandbox.slurm.time_limit && <KV label="Time Limit" value={sandbox.slurm.time_limit} />}
          {sandbox.slurm.modules && sandbox.slurm.modules.length > 0 && (
            <KV label="Modules" value={sandbox.slurm.modules.join(", ")} />
          )}
        </>
      )}

      {sandbox.target === "cloud_function" && sandbox.cloud_function && (
        <>
          <KV label="Provider" value={sandbox.cloud_function.provider.replace("_", " ").toUpperCase()} />
          <KV label="Function" value={sandbox.cloud_function.function_name} />
          <KV label="Region" value={sandbox.cloud_function.region} />
          {sandbox.cloud_function.runtime && <KV label="Runtime" value={sandbox.cloud_function.runtime} />}
          {sandbox.cloud_function.memory && <KV label="Memory" value={`${sandbox.cloud_function.memory}MB`} />}
        </>
      )}

      {sandbox.target === "kubernetes" && sandbox.kubernetes && (
        <>
          <KV label="Cluster" value={sandbox.kubernetes.cluster} />
          <KV label="Namespace" value={sandbox.kubernetes.namespace} />
          <KV label="Image" value={sandbox.kubernetes.image} />
          <KV label="Command" value={sandbox.kubernetes.command} />
          {sandbox.kubernetes.cpu_limit && <KV label="CPU Limit" value={sandbox.kubernetes.cpu_limit} />}
          {sandbox.kubernetes.memory_limit && <KV label="Memory Limit" value={sandbox.kubernetes.memory_limit} />}
          {sandbox.kubernetes.service_account && <KV label="Service Account" value={sandbox.kubernetes.service_account} />}
        </>
      )}
    </div>
  );
}

function KV({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div style={{ display: "flex", gap: 8 }}>
      <span style={{ color: "var(--celerex-text-secondary)", minWidth: 100, flexShrink: 0 }}>{label}:</span>
      <span style={{
        color: "var(--celerex-text-primary)",
        fontFamily: mono ? "monospace" : "inherit",
        fontSize: mono ? 11 : 12,
        wordBreak: "break-all",
      }}>{value}</span>
    </div>
  );
}

const labelStyle: React.CSSProperties = {
  display: "block",
  fontSize: 12,
  fontWeight: 600,
  color: "var(--celerex-text-secondary)",
  marginBottom: 4,
  marginTop: 12,
};

const inputStyle: React.CSSProperties = {
  display: "block",
  width: "100%",
  boxSizing: "border-box",
  padding: "8px 10px",
  borderRadius: 6,
  border: "1px solid var(--celerex-border)",
  background: "var(--celerex-bg-primary)",
  color: "var(--celerex-text-primary)",
  fontSize: 13,
  outline: "none",
  marginBottom: 4,
};
