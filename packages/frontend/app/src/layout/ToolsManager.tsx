import { useState, useMemo } from "react";
import {
  getAllTools,
  searchTools,
  addCustomTool,
  updateCustomTool,
  deleteCustomTool,
  TOOL_CATEGORIES,
  type ToolDef,
  type ToolCategoryId,
  type ToolParameter,
} from "../data/tools";

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
