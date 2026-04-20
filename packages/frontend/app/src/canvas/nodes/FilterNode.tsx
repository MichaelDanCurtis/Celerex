import { Handle, Position, NodeProps, Node } from "@xyflow/react";
import { CATEGORY_COLORS } from "../palette";
import { StepState } from "../types";

export interface FilterNodeData extends Record<string, unknown> {
  label: string;
  state?: StepState;
}

export type FilterNodeType = Node<FilterNodeData, "filter">;

export function FilterNode({ data }: NodeProps<FilterNodeType>) {
  const c = CATEGORY_COLORS.filter;
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
      <Handle type="source" position={Position.Right} />
    </div>
  );
}
