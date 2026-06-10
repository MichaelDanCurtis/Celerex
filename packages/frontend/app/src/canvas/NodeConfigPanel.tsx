import { useState } from "react";
import { GraphNode } from "./types";
import { CATEGORY_META, iconFor } from "./palette";

const PROVIDERS = ["default", "anthropic", "openai", "kimi", "gemini"];

export function NodeConfigPanel({
  node,
  onChange,
  upstreamIds = [],
}: {
  node: GraphNode | null;
  onChange: (next: GraphNode) => void;
  upstreamIds?: string[];
}) {
  const [advanced, setAdvanced] = useState(false);

  if (!node) {
    return (
      <div
        data-testid="config-panel"
        style={{ padding: 16, color: "var(--celerex-text-secondary)", fontSize: 13 }}
      >
        <div style={{ fontSize: 28, opacity: 0.4, marginBottom: 8 }}>✦</div>
        Select a node on the canvas to configure it.
      </div>
    );
  }

  const config = node.config ?? {};
  const exec = String(config.execution ?? "");
  const accent = node.type ? CATEGORY_META[node.type].color : "#a78bfa";

  const patch = (p: Record<string, unknown>) =>
    onChange({ ...node, config: { ...config, ...p } });

  return (
    <div
      data-testid="config-panel"
      style={{ padding: 14, overflowY: "auto", fontSize: 13 }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 12 }}>
        <span style={{ fontSize: 18 }}>{iconFor(node.type, config)}</span>
        <div>
          <div style={{ fontSize: 10, textTransform: "uppercase", letterSpacing: 1, color: accent, fontWeight: 700 }}>
            {node.type ? CATEGORY_META[node.type].label : "Node"}
          </div>
          <code style={{ fontSize: 11, color: "var(--celerex-text-secondary)" }}>{node.id}</code>
        </div>
      </div>

      <Field label="Label">
        <Input value={String(config.label ?? "")} onChange={(v) => patch({ label: v })} />
      </Field>

      {node.type === "filter" ? (
        <Field label="Condition" hint="Gate downstream nodes. e.g. {{steps.metrics.outputs.count}} > 5">
          <Input
            value={String(config.expression ?? "")}
            onChange={(v) => patch({ expression: v })}
            mono
          />
        </Field>
      ) : exec === "llm" ? (
        <LlmForm config={config} patch={patch} />
      ) : exec === "http" ? (
        <HttpForm config={config} patch={patch} />
      ) : exec === "script" ? (
        <ScriptForm config={config} patch={patch} />
      ) : exec === "doc" ? (
        <DocForm config={config} patch={patch} />
      ) : exec === "memory" ? (
        <MemoryForm config={config} patch={patch} />
      ) : exec === "galvo" ? (
        <GalvoForm config={config} patch={patch} />
      ) : exec === "m365" ? (
        <M365Form config={config} patch={patch} />
      ) : exec === "input" ? (
        <JsonField
          label="Payload"
          hint="Emitted downstream as this trigger's output."
          value={config.inputs ?? {}}
          onChange={(v) => patch({ inputs: v })}
        />
      ) : (
        <div style={{ color: "var(--celerex-text-secondary)", fontSize: 12, marginTop: 8 }}>
          Use Advanced JSON below to configure this node.
        </div>
      )}

      {upstreamIds.length > 0 && <WireHelper ids={upstreamIds} />}

      <button
        onClick={() => setAdvanced((a) => !a)}
        style={{
          marginTop: 16,
          background: "transparent",
          border: "none",
          color: "var(--celerex-accent)",
          cursor: "pointer",
          fontSize: 12,
          padding: 0,
        }}
      >
        {advanced ? "▾ Hide advanced JSON" : "▸ Advanced JSON"}
      </button>
      {advanced && (
        <textarea
          value={JSON.stringify(config, null, 2)}
          rows={12}
          onChange={(e) => {
            try {
              onChange({ ...node, config: JSON.parse(e.target.value) });
            } catch {
              /* ignore parse errors mid-typing */
            }
          }}
          style={{
            width: "100%",
            marginTop: 8,
            fontFamily: "ui-monospace, Menlo, monospace",
            fontSize: 11.5,
            padding: 8,
            borderRadius: 8,
            border: "1px solid var(--celerex-border)",
            background: "var(--celerex-bg-secondary)",
            color: "var(--celerex-text-primary)",
            resize: "vertical",
          }}
        />
      )}
    </div>
  );
}

// ---- Per-type forms -------------------------------------------------------

type Patch = (p: Record<string, unknown>) => void;
type Cfg = Record<string, unknown>;

function LlmForm({ config, patch }: { config: Cfg; patch: Patch }) {
  const messages = (config.inputs as { messages?: Array<{ role: string; content: string }> } | undefined)?.messages ?? [];
  const system = messages.find((m) => m.role === "system")?.content ?? "";
  const user = messages.find((m) => m.role === "user")?.content ?? "";
  const setMessages = (sys: string, usr: string) => {
    const next = [
      ...(sys.trim() ? [{ role: "system", content: sys }] : []),
      { role: "user", content: usr },
    ];
    patch({ inputs: { ...(config.inputs as object), messages: next } });
  };
  return (
    <>
      <Field label="Provider">
        <Select
          value={String(config.provider_id ?? "default")}
          options={PROVIDERS}
          onChange={(v) => patch({ provider_id: v })}
        />
      </Field>
      <Field label="Model">
        <Input value={String(config.model ?? "")} onChange={(v) => patch({ model: v })} mono />
      </Field>
      <Field label="System prompt" hint="Optional. Sets the agent's role.">
        <TextArea value={system} onChange={(v) => setMessages(v, user)} rows={3} />
      </Field>
      <Field label="Prompt" hint="Reference upstream with {{steps.id.outputs}}.">
        <TextArea value={user} onChange={(v) => setMessages(system, v)} rows={5} />
      </Field>
    </>
  );
}

function HttpForm({ config, patch }: { config: Cfg; patch: Patch }) {
  const method = String(config.method ?? "GET");
  return (
    <>
      <Field label="Method">
        <Select
          value={method}
          options={["GET", "POST", "PUT", "PATCH", "DELETE"]}
          onChange={(v) => patch({ method: v })}
        />
      </Field>
      <Field label="URL">
        <Input value={String(config.url ?? "")} onChange={(v) => patch({ url: v })} mono />
      </Field>
      <JsonField label="Headers" value={config.headers ?? {}} onChange={(v) => patch({ headers: v })} />
      {method !== "GET" && (
        <JsonField
          label="Body (inputs)"
          hint="Sent as the JSON request body."
          value={config.inputs ?? {}}
          onChange={(v) => patch({ inputs: v })}
        />
      )}
    </>
  );
}

function ScriptForm({ config, patch }: { config: Cfg; patch: Patch }) {
  const args = Array.isArray(config.args) ? (config.args as string[]) : [];
  return (
    <>
      <Field label="Command">
        <Input value={String(config.command ?? "")} onChange={(v) => patch({ command: v })} mono />
      </Field>
      <Field label="Arguments" hint="One per line.">
        <TextArea
          value={args.join("\n")}
          onChange={(v) => patch({ args: v.split("\n").filter((s) => s.length > 0) })}
          rows={3}
          mono
        />
      </Field>
    </>
  );
}

function DocForm({ config, patch }: { config: Cfg; patch: Patch }) {
  const op = String(config.op ?? "append");
  const content = (config.inputs as { markdown?: string } | undefined)?.markdown ?? "";
  return (
    <>
      <Field label="Operation">
        <Select
          value={op}
          options={["create", "append", "read"]}
          onChange={(v) => patch({ op: v })}
        />
      </Field>
      {op === "create" && (
        <Field label="Title" hint="Supports {{steps...}} references.">
          <Input value={String(config.title ?? "")} onChange={(v) => patch({ title: v })} />
        </Field>
      )}
      {(op === "append" || op === "read") && (
        <Field label="Document ID" hint="UUID, or {{steps.create.outputs.document_id}}.">
          <Input value={String(config.document_id ?? "")} onChange={(v) => patch({ document_id: v })} mono />
        </Field>
      )}
      {(op === "create" || op === "append") && (
        <Field label="Content (markdown)" hint="Usually {{steps.agent.outputs}}.">
          <TextArea
            value={content}
            onChange={(v) => patch({ inputs: { ...(config.inputs as object), markdown: v } })}
            rows={5}
            mono
          />
        </Field>
      )}
    </>
  );
}

const MEMORY_LABELS = [
  "Entity",
  "Decision",
  "Pattern",
  "Belief",
  "Artifact",
  "Session",
  "Task",
  "Failure",
  "Convention",
  "Constraint",
];

function MemoryForm({ config, patch }: { config: Cfg; patch: Patch }) {
  const op = String(config.op ?? "search");
  return (
    <>
      <Field label="Operation" hint="galvo-memory (Neo4j) — recall or store.">
        <Select value={op} options={["search", "store"]} onChange={(v) => patch({ op: v })} />
      </Field>
      <Field label="Memory type" hint="The galvo-memory node label.">
        <Select
          value={String(config.mem_label ?? (op === "store" ? "Decision" : "Entity"))}
          options={op === "search" ? MEMORY_LABELS : MEMORY_LABELS.filter((l) => l !== "Entity")}
          onChange={(v) => patch({ mem_label: v })}
        />
      </Field>
      <Field label="Scope" hint="Default: project:celerex">
        <Input value={String(config.scope ?? "")} onChange={(v) => patch({ scope: v })} mono />
      </Field>
      {op === "search" ? (
        <>
          <Field label="Query" hint="Supports {{steps.id.outputs}}.">
            <TextArea value={String(config.query ?? "")} onChange={(v) => patch({ query: v })} rows={3} />
          </Field>
          <Field label="Limit">
            <Input value={String(config.limit ?? 5)} onChange={(v) => patch({ limit: Number(v) || 5 })} />
          </Field>
        </>
      ) : (
        <JsonField
          label="Fields"
          hint="Per-label properties, e.g. { title, rationale, confidence }."
          value={config.fields ?? {}}
          onChange={(v) => patch({ fields: v })}
        />
      )}
    </>
  );
}

function GalvoForm({ config, patch }: { config: Cfg; patch: Patch }) {
  return (
    <>
      <Field label="Agent handle" hint="Galvo agent name (control plane).">
        <Input value={String(config.agent ?? "")} onChange={(v) => patch({ agent: v })} mono />
      </Field>
      <Field label="Input" hint="Sent as the agent's input. Supports {{steps...}}.">
        <TextArea value={String(config.input ?? "")} onChange={(v) => patch({ input: v })} rows={4} />
      </Field>
      <Field label="Token" hint="Optional. Else GALVO_CONTROL_PLANE_TOKEN env.">
        <Input value={String(config.token ?? "")} onChange={(v) => patch({ token: v })} mono />
      </Field>
      <div style={{ fontSize: 10.5, color: "var(--celerex-text-secondary)", marginTop: 4 }}>
        Targets GALVO_CONTROL_PLANE_URL (default http://localhost:8787).
      </div>
    </>
  );
}

function M365Form({ config, patch }: { config: Cfg; patch: Patch }) {
  const op = String(config.op ?? "copilot_publish");
  return (
    <>
      <Field label="Operation">
        <Select
          value={op}
          options={["copilot_publish", "copilot_setup", "graph_get"]}
          onChange={(v) => patch({ op: v })}
        />
      </Field>
      {op === "graph_get" && (
        <Field label="Graph path" hint="e.g. /users/{id}/messages or /sites/{id}/drive">
          <Input value={String(config.path ?? "")} onChange={(v) => patch({ path: v })} mono />
        </Field>
      )}
      {(op === "copilot_publish" || op === "copilot_setup") && (
        <Field label="Connection ID">
          <Input value={String(config.connection_id ?? "")} onChange={(v) => patch({ connection_id: v })} mono />
        </Field>
      )}
      {op === "copilot_setup" && (
        <>
          <Field label="Name">
            <Input value={String(config.name ?? "")} onChange={(v) => patch({ name: v })} />
          </Field>
          <Field label="Description">
            <Input value={String(config.description ?? "")} onChange={(v) => patch({ description: v })} />
          </Field>
        </>
      )}
      {op === "copilot_publish" && (
        <>
          <Field label="Item ID">
            <Input value={String(config.item_id ?? "")} onChange={(v) => patch({ item_id: v })} mono />
          </Field>
          <Field label="Title">
            <Input value={String(config.title ?? "")} onChange={(v) => patch({ title: v })} />
          </Field>
          <Field label="Content" hint="Usually {{steps.agent.outputs}}.">
            <TextArea value={String(config.content ?? "")} onChange={(v) => patch({ content: v })} rows={4} mono />
          </Field>
        </>
      )}
      <div style={{ fontSize: 10.5, color: "var(--celerex-text-secondary)", marginTop: 4 }}>
        Auth via AZURE_TENANT_ID / AZURE_CLIENT_ID / AZURE_CLIENT_SECRET env (app-only).
      </div>
    </>
  );
}

function WireHelper({ ids }: { ids: string[] }) {
  const [copied, setCopied] = useState<string | null>(null);
  return (
    <div style={{ marginTop: 16, paddingTop: 12, borderTop: "1px solid var(--celerex-border)" }}>
      <div style={{ fontSize: 11, fontWeight: 700, textTransform: "uppercase", letterSpacing: 0.6, color: "var(--celerex-text-secondary)", marginBottom: 8 }}>
        Wire data from
      </div>
      <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
        {ids.map((id) => {
          const token = `{{steps.${id}.outputs}}`;
          return (
            <button
              key={id}
              onClick={() => {
                void navigator.clipboard?.writeText(token);
                setCopied(id);
                setTimeout(() => setCopied((c) => (c === id ? null : c)), 1200);
              }}
              title={`Copy ${token}`}
              style={{
                fontSize: 11,
                fontFamily: "ui-monospace, Menlo, monospace",
                padding: "3px 7px",
                borderRadius: 6,
                border: "1px solid var(--celerex-border)",
                background: "var(--celerex-bg-secondary)",
                color: "var(--celerex-text-primary)",
                cursor: "pointer",
              }}
            >
              {copied === id ? "copied!" : id}
            </button>
          );
        })}
      </div>
    </div>
  );
}

// ---- Tiny styled controls -------------------------------------------------

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div style={{ marginBottom: 12 }}>
      <label style={{ display: "block", fontSize: 11, fontWeight: 600, marginBottom: 4, color: "var(--celerex-text-secondary)" }}>
        {label}
      </label>
      {children}
      {hint && <div style={{ fontSize: 10.5, color: "var(--celerex-text-secondary)", opacity: 0.8, marginTop: 3 }}>{hint}</div>}
    </div>
  );
}

const baseControl: React.CSSProperties = {
  width: "100%",
  padding: "6px 8px",
  borderRadius: 7,
  border: "1px solid var(--celerex-border)",
  background: "var(--celerex-bg-primary)",
  color: "var(--celerex-text-primary)",
  fontSize: 13,
};

function Input({ value, onChange, mono }: { value: string; onChange: (v: string) => void; mono?: boolean }) {
  return (
    <input
      value={value}
      onChange={(e) => onChange(e.target.value)}
      style={{ ...baseControl, fontFamily: mono ? "ui-monospace, Menlo, monospace" : undefined, fontSize: mono ? 12 : 13 }}
    />
  );
}

function TextArea({ value, onChange, rows = 3, mono }: { value: string; onChange: (v: string) => void; rows?: number; mono?: boolean }) {
  return (
    <textarea
      value={value}
      rows={rows}
      onChange={(e) => onChange(e.target.value)}
      style={{ ...baseControl, resize: "vertical", fontFamily: mono ? "ui-monospace, Menlo, monospace" : "inherit", fontSize: mono ? 12 : 13 }}
    />
  );
}

function Select({ value, options, onChange }: { value: string; options: string[]; onChange: (v: string) => void }) {
  return (
    <select value={value} onChange={(e) => onChange(e.target.value)} style={baseControl}>
      {options.map((o) => (
        <option key={o} value={o}>
          {o}
        </option>
      ))}
    </select>
  );
}

function JsonField({ label, hint, value, onChange }: { label: string; hint?: string; value: unknown; onChange: (v: unknown) => void }) {
  const [text, setText] = useState(() => JSON.stringify(value ?? {}, null, 2));
  const [err, setErr] = useState(false);
  return (
    <Field label={label} hint={hint}>
      <textarea
        value={text}
        rows={4}
        onChange={(e) => {
          setText(e.target.value);
          try {
            onChange(JSON.parse(e.target.value));
            setErr(false);
          } catch {
            setErr(true);
          }
        }}
        style={{
          ...baseControl,
          fontFamily: "ui-monospace, Menlo, monospace",
          fontSize: 12,
          resize: "vertical",
          borderColor: err ? "#ef4444" : "var(--celerex-border)",
        }}
      />
    </Field>
  );
}
