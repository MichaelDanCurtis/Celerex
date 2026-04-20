import { Handle, Position, NodeProps, Node } from "@xyflow/react";
import { CATEGORY_COLORS } from "../palette";
import { StepState } from "../types";

export interface OutboundNodeData extends Record<string, unknown> {
  label: string;
  state?: StepState;
}

export type OutboundNodeType = Node<OutboundNodeData, "outbound">;

export function OutboundNode({ data }: NodeProps<OutboundNodeType>) {
  const c = CATEGORY_COLORS.outbound;
  return (
    <div
      data-testid={`node-${data.label}`}
      style={{
        padding: "8px 12px",
        borderRadius: 8,
        background: c.bg,
        border: `2px solid ${c.border}`,
        minWidth: 140,
        fontSize: 13,
      }}
    >
      <Handle type="target" position={Position.Left} />
      {data.label}
    </div>
  );
}
