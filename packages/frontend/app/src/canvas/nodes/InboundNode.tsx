import { Handle, Position, NodeProps, Node } from "@xyflow/react";
import { CATEGORY_COLORS } from "../palette";
import { StepState } from "../types";

export interface InboundNodeData extends Record<string, unknown> {
  label: string;
  state?: StepState;
}

export type InboundNodeType = Node<InboundNodeData, "inbound">;

export function InboundNode({ data }: NodeProps<InboundNodeType>) {
  const c = CATEGORY_COLORS.inbound;
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
      {data.label}
      <Handle type="source" position={Position.Right} />
    </div>
  );
}
