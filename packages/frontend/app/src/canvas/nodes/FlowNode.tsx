import { Handle, Position, NodeProps, Node } from "@xyflow/react";
import { CATEGORY_META, accentFor, iconFor } from "../palette";
import { FlowNodeData, NodeCategory, StepState } from "../types";

export type FlowNodeType = Node<FlowNodeData>;

/** One node renderer for all four categories — LangFlow-style cards. */
export function FlowNode({ data, selected }: NodeProps<FlowNodeType>) {
  const category: NodeCategory = data.category ?? "action";
  const config = data.config ?? {};
  const accent = accentFor(category, config);
  const icon = iconFor(category, config);
  const meta = CATEGORY_META[category];
  const state = data.state ?? "idle";

  const ring = stateRing(state, accent, meta.glow, selected);

  return (
    <div
      data-testid={`node-${data.label}`}
      style={{
        width: 210,
        borderRadius: 12,
        background: "linear-gradient(180deg, #1c2130 0%, #161a26 100%)",
        border: `1px solid ${ring.border}`,
        boxShadow: ring.shadow,
        color: "#e6e9f0",
        fontSize: 13,
        overflow: "hidden",
        transition: "box-shadow 120ms ease, border-color 120ms ease",
        animation: state === "running" ? "celerex-pulse 1.2s ease-in-out infinite" : undefined,
      }}
    >
      {category !== "inbound" && (
        <Handle
          type="target"
          position={Position.Left}
          style={handleStyle(accent)}
        />
      )}

      {/* Header */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          padding: "8px 10px",
          background: `linear-gradient(90deg, ${hexA(accent, 0.22)}, ${hexA(accent, 0.04)})`,
          borderBottom: `1px solid ${hexA(accent, 0.25)}`,
        }}
      >
        <span
          style={{
            width: 26,
            height: 26,
            borderRadius: 8,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            fontSize: 15,
            background: hexA(accent, 0.18),
            border: `1px solid ${hexA(accent, 0.4)}`,
          }}
        >
          {icon}
        </span>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div
            style={{
              fontSize: 9,
              letterSpacing: 1,
              textTransform: "uppercase",
              color: accent,
              fontWeight: 700,
            }}
          >
            {meta.label}
          </div>
          <div
            style={{
              fontWeight: 600,
              whiteSpace: "nowrap",
              overflow: "hidden",
              textOverflow: "ellipsis",
            }}
          >
            {data.label}
          </div>
        </div>
        <StateGlyph state={state} />
      </div>

      {/* Body — config summary */}
      <div
        style={{
          padding: "8px 10px",
          fontSize: 11,
          color: "#9aa3b8",
          fontFamily:
            "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
          whiteSpace: "nowrap",
          overflow: "hidden",
          textOverflow: "ellipsis",
        }}
        title={configSummary(category, config)}
      >
        {configSummary(category, config)}
      </div>

      {category !== "outbound" && (
        <Handle
          type="source"
          position={Position.Right}
          style={handleStyle(accent)}
        />
      )}
    </div>
  );
}

function StateGlyph({ state }: { state: StepState }) {
  if (state === "running")
    return <span style={{ color: "#60a5fa", fontSize: 14 }}>◌</span>;
  if (state === "succeeded")
    return <span style={{ color: "#34d399", fontSize: 14 }}>✓</span>;
  if (state === "failed")
    return <span style={{ color: "#f87171", fontSize: 14 }}>✕</span>;
  return null;
}

function handleStyle(accent: string): React.CSSProperties {
  return {
    width: 11,
    height: 11,
    background: "#0f1117",
    border: `2px solid ${accent}`,
    boxShadow: `0 0 6px ${hexA(accent, 0.6)}`,
  };
}

function stateRing(
  state: StepState,
  accent: string,
  glow: string,
  selected: boolean,
): { border: string; shadow: string } {
  if (state === "running")
    return { border: "#60a5fa", shadow: `0 0 0 1px #60a5fa, 0 6px 22px rgba(96,165,250,0.35)` };
  if (state === "succeeded")
    return { border: "#34d399", shadow: `0 0 0 1px #34d399, 0 6px 18px rgba(52,211,153,0.25)` };
  if (state === "failed")
    return { border: "#f87171", shadow: `0 0 0 1px #f87171, 0 6px 18px rgba(248,113,113,0.3)` };
  if (selected)
    return { border: accent, shadow: `0 0 0 1px ${accent}, 0 8px 24px ${glow}` };
  return { border: "rgba(255,255,255,0.08)", shadow: "0 4px 14px rgba(0,0,0,0.4)" };
}

function configSummary(category: NodeCategory, config: Record<string, unknown>): string {
  if (category === "filter") {
    return String(config.expression ?? "no condition");
  }
  const exec = String(config.execution ?? "");
  switch (exec) {
    case "llm":
      return `model: ${config.model ?? "default"}`;
    case "http":
      return `${String(config.method ?? "GET")} ${truncate(String(config.url ?? ""), 22)}`;
    case "script":
      return `$ ${truncate(String(config.command ?? ""), 24)}`;
    case "doc": {
      const op = String(config.op ?? "append");
      const tgt = op === "create" ? String(config.title ?? "") : String(config.document_id ?? "");
      return `doc · ${op}${tgt ? ` · ${truncate(tgt, 16)}` : ""}`;
    }
    case "input":
      return "trigger";
    default:
      return String(config.template ?? exec ?? "node");
  }
}

function truncate(s: string, n: number): string {
  return s.length > n ? `${s.slice(0, n - 1)}…` : s;
}

/** Expand a hex colour to rgba() with the given alpha. */
function hexA(hex: string, alpha: number): string {
  const h = hex.replace("#", "");
  const full = h.length === 3 ? h.split("").map((c) => c + c).join("") : h;
  const r = parseInt(full.slice(0, 2), 16);
  const g = parseInt(full.slice(2, 4), 16);
  const b = parseInt(full.slice(4, 6), 16);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}
