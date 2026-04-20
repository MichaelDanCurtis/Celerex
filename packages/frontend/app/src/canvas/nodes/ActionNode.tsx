import { Handle, Position, NodeProps, Node } from "@xyflow/react";
import { CATEGORY_COLORS } from "../palette";
import { StepState } from "../types";

export interface ActionNodeData extends Record<string, unknown> {
  label: string;
  state?: StepState;
}

export type ActionNodeType = Node<ActionNodeData, "action">;

export function ActionNode({ data }: NodeProps<ActionNodeType>) {
  const c = CATEGORY_COLORS.action;
  return (
    <div
      data-testid={`node-${data.label}`}
      style={{
        padding: "8px 12px",
        borderRadius: 8,
        background: c.bg,
        border: `2px solid ${stateBorder(data.state) ?? c.border}`,
        minWidth: 140,
        fontSize: 13,
      }}
    >
      <Handle type="target" position={Position.Left} />
      {data.label}
      {data.state === "running" && <span style={{ marginLeft: 6 }}>⏳</span>}
      {data.state === "succeeded" && <span style={{ marginLeft: 6 }}>✓</span>}
      {data.state === "failed" && <span style={{ marginLeft: 6 }}>✗</span>}
      <Handle type="source" position={Position.Right} />
    </div>
  );
}

function stateBorder(s?: StepState): string | undefined {
  return s === "running"
    ? "#3b82f6"
    : s === "succeeded"
      ? "#16a34a"
      : s === "failed"
        ? "#dc2626"
        : undefined;
}
