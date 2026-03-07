import React, { useState, useEffect, useCallback } from "react";
import {
  IO_TYPES,
  CATEGORIES,
  type AgentDef,
  type IOTypeId,
  type CategoryId,
} from "../data/agents";

export interface AgentEditorDialogProps {
  open: boolean;
  onClose: () => void;
  /** If provided, we're editing. If null, we're creating. */
  agent: AgentDef | null;
  onSave: (agent: AgentDef) => void;
  onDelete?: (id: string) => void;
}

const defaultAgent = (): {
  name: string;
  description: string;
  category: CategoryId;
  status: "active" | "idle" | "disabled";
  inputs: IOTypeId[];
  outputs: IOTypeId[];
  systemPrompt: string;
  model: string;
} => ({
  name: "",
  description: "",
  category: "document-management",
  status: "active",
  inputs: ["text"],
  outputs: ["text"],
  systemPrompt: "",
  model: "",
});

export function AgentEditorDialog({
  open,
  onClose,
  agent,
  onSave,
  onDelete,
}: AgentEditorDialogProps) {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [category, setCategory] = useState<CategoryId>("document-management");
  const [status, setStatus] = useState<"active" | "idle" | "disabled">("active");
  const [inputs, setInputs] = useState<IOTypeId[]>(["text"]);
  const [outputs, setOutputs] = useState<IOTypeId[]>(["text"]);
  const [systemPrompt, setSystemPrompt] = useState("");
  const [model, setModel] = useState("");

  // Initialize state when dialog opens
  useEffect(() => {
    if (!open) return;
    if (agent) {
      setName(agent.name);
      setDescription(agent.description);
      setCategory(agent.category);
      setStatus(agent.status);
      setInputs(agent.inputs ?? ["text"]);
      setOutputs(agent.outputs ?? ["text"]);
      setSystemPrompt(agent.systemPrompt ?? "");
      setModel(agent.model ?? "");
    } else {
      const d = defaultAgent();
      setName(d.name);
      setDescription(d.description);
      setCategory(d.category);
      setStatus(d.status);
      setInputs(d.inputs);
      setOutputs(d.outputs);
      setSystemPrompt(d.systemPrompt);
      setModel(d.model);
    }
  }, [open, agent]);

  // Escape key handler
  useEffect(() => {
    if (!open) return;
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [open, onClose]);

  const toggleIO = useCallback(
    (list: IOTypeId[], setList: React.Dispatch<React.SetStateAction<IOTypeId[]>>, id: IOTypeId) => {
      setList((prev) => (prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id]));
    },
    []
  );

  const handleSave = () => {
    const result: AgentDef = {
      id: agent?.id ?? "custom-" + Date.now().toString(36),
      name: name.trim(),
      description: description.trim(),
      category,
      status,
      inputs,
      outputs,
      systemPrompt: systemPrompt.trim() || undefined,
      model: model.trim() || undefined,
      isCustom: agent?.isCustom ?? true,
    };
    onSave(result);
  };

  const handleDelete = () => {
    if (agent && onDelete) {
      onDelete(agent.id);
    }
  };

  if (!open) return null;

  const isEditing = agent !== null;

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 10000,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        backgroundColor: "rgba(0, 0, 0, 0.55)",
      }}
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        style={{
          width: "100%",
          maxWidth: 600,
          maxHeight: "85vh",
          overflowY: "auto",
          backgroundColor: "var(--celerex-bg-primary)",
          border: "1px solid var(--celerex-border)",
          borderRadius: 12,
          boxShadow: "0 20px 60px rgba(0, 0, 0, 0.4)",
          display: "flex",
          flexDirection: "column",
        }}
      >
        {/* Header */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            padding: "16px 20px",
            borderBottom: "1px solid var(--celerex-border)",
          }}
        >
          <h2
            style={{
              margin: 0,
              fontSize: 18,
              fontWeight: 600,
              color: "var(--celerex-text-primary)",
            }}
          >
            {isEditing ? "Edit Agent" : "Create Agent"}
          </h2>
          <button
            onClick={onClose}
            style={{
              background: "none",
              border: "none",
              color: "var(--celerex-text-secondary)",
              fontSize: 20,
              cursor: "pointer",
              padding: "4px 8px",
              borderRadius: 4,
              lineHeight: 1,
            }}
          >
            ✕
          </button>
        </div>

        {/* Body */}
        <div style={{ padding: "20px", display: "flex", flexDirection: "column", gap: 20 }}>
          {/* Basic Info */}
          <section style={{ display: "flex", flexDirection: "column", gap: 12 }}>
            <SectionLabel>Basic Info</SectionLabel>

            <FieldGroup label="Name">
              <input
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                required
                placeholder="Agent name"
                style={inputStyle}
              />
            </FieldGroup>

            <FieldGroup label="Description">
              <textarea
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                rows={2}
                placeholder="What does this agent do?"
                style={{ ...inputStyle, resize: "vertical" }}
              />
            </FieldGroup>

            <div style={{ display: "flex", gap: 12 }}>
              <FieldGroup label="Category" style={{ flex: 1 }}>
                <select
                  value={category}
                  onChange={(e) => setCategory(e.target.value as CategoryId)}
                  style={inputStyle}
                >
                  {CATEGORIES.map((c) => (
                    <option key={c.id} value={c.id}>
                      {c.label}
                    </option>
                  ))}
                </select>
              </FieldGroup>

              <FieldGroup label="Status" style={{ flex: 1 }}>
                <select
                  value={status}
                  onChange={(e) => setStatus(e.target.value as "active" | "idle" | "disabled")}
                  style={inputStyle}
                >
                  <option value="active">Active</option>
                  <option value="idle">Idle</option>
                  <option value="disabled">Disabled</option>
                </select>
              </FieldGroup>
            </div>
          </section>

          {/* Input/Output Configuration */}
          <section style={{ display: "flex", flexDirection: "column", gap: 12 }}>
            <SectionLabel>Input / Output Configuration</SectionLabel>

            <FieldGroup label="Inputs">
              <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
                {IO_TYPES.map((ioType) => {
                  const selected = inputs.includes(ioType.id);
                  return (
                    <IOChip
                      key={ioType.id}
                      icon={ioType.icon}
                      label={ioType.label}
                      color={ioType.color}
                      selected={selected}
                      onClick={() => toggleIO(inputs, setInputs, ioType.id)}
                    />
                  );
                })}
              </div>
            </FieldGroup>

            <FieldGroup label="Outputs">
              <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
                {IO_TYPES.map((ioType) => {
                  const selected = outputs.includes(ioType.id);
                  return (
                    <IOChip
                      key={ioType.id}
                      icon={ioType.icon}
                      label={ioType.label}
                      color={ioType.color}
                      selected={selected}
                      onClick={() => toggleIO(outputs, setOutputs, ioType.id)}
                    />
                  );
                })}
              </div>
            </FieldGroup>
          </section>

          {/* Agent Behavior */}
          <section style={{ display: "flex", flexDirection: "column", gap: 12 }}>
            <SectionLabel>Agent Behavior</SectionLabel>

            <FieldGroup label="System Prompt">
              <textarea
                value={systemPrompt}
                onChange={(e) => setSystemPrompt(e.target.value)}
                rows={5}
                placeholder="Instructions for this agent..."
                style={{ ...inputStyle, fontFamily: "monospace", resize: "vertical" }}
              />
            </FieldGroup>

            <FieldGroup label="Model Override">
              <input
                type="text"
                value={model}
                onChange={(e) => setModel(e.target.value)}
                placeholder="Leave blank for default"
                style={inputStyle}
              />
            </FieldGroup>
          </section>
        </div>

        {/* Footer */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "flex-end",
            gap: 10,
            padding: "16px 20px",
            borderTop: "1px solid var(--celerex-border)",
          }}
        >
          {isEditing && agent?.isCustom && onDelete && (
            <button
              onClick={handleDelete}
              style={{
                marginRight: "auto",
                padding: "8px 16px",
                borderRadius: 6,
                border: "1px solid #ef4444",
                background: "transparent",
                color: "#ef4444",
                fontSize: 13,
                fontWeight: 500,
                cursor: "pointer",
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
              color: "var(--celerex-text-secondary)",
              fontSize: 13,
              fontWeight: 500,
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
              background: name.trim() ? "var(--celerex-accent)" : "var(--celerex-bg-secondary)",
              color: name.trim() ? "#fff" : "var(--celerex-text-secondary)",
              fontSize: 13,
              fontWeight: 600,
              cursor: name.trim() ? "pointer" : "not-allowed",
              opacity: name.trim() ? 1 : 0.6,
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
/*  Sub-components                                                     */
/* ------------------------------------------------------------------ */

function SectionLabel({ children }: { children: React.ReactNode }) {
  return (
    <div
      style={{
        fontSize: 13,
        fontWeight: 600,
        color: "var(--celerex-text-secondary)",
        textTransform: "uppercase",
        letterSpacing: "0.05em",
      }}
    >
      {children}
    </div>
  );
}

function FieldGroup({
  label,
  children,
  style,
}: {
  label: string;
  children: React.ReactNode;
  style?: React.CSSProperties;
}) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 4, ...style }}>
      <label
        style={{
          fontSize: 12,
          fontWeight: 500,
          color: "var(--celerex-text-secondary)",
        }}
      >
        {label}
      </label>
      {children}
    </div>
  );
}

function IOChip({
  icon,
  label,
  color,
  selected,
  onClick,
}: {
  icon: string;
  label: string;
  color: string;
  selected: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 4,
        height: 28,
        padding: "0 10px",
        borderRadius: 14,
        border: `1.5px solid ${color}`,
        background: selected ? color : "transparent",
        color: selected ? "#fff" : color,
        fontSize: 12,
        fontWeight: 500,
        cursor: "pointer",
        whiteSpace: "nowrap",
        transition: "all 0.15s ease",
      }}
    >
      <span style={{ fontSize: 11 }}>{icon}</span>
      {label}
    </button>
  );
}

/* ------------------------------------------------------------------ */
/*  Shared styles                                                      */
/* ------------------------------------------------------------------ */

const inputStyle: React.CSSProperties = {
  width: "100%",
  padding: "8px 10px",
  borderRadius: 6,
  border: "1px solid var(--celerex-border)",
  background: "var(--celerex-bg-secondary)",
  color: "var(--celerex-text-primary)",
  fontSize: 13,
  outline: "none",
  boxSizing: "border-box",
};
