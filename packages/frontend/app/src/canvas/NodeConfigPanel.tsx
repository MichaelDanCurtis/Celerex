import { GraphNode } from "./types";

export function NodeConfigPanel({
  node,
  onChange,
}: {
  node: GraphNode | null;
  onChange: (next: GraphNode) => void;
}) {
  if (!node) {
    return (
      <div style={{ width: 280, padding: 12, opacity: 0.5 }}>Select a node</div>
    );
  }
  return (
    <div
      data-testid="config-panel"
      style={{ width: 280, padding: 12, borderLeft: "1px solid var(--celerex-border)" }}
    >
      <h4>{node.id}</h4>
      <textarea
        value={JSON.stringify(node.config, null, 2)}
        rows={12}
        onChange={(e) => {
          try {
            onChange({ ...node, config: JSON.parse(e.target.value) });
          } catch {
            /* ignore parse errors mid-typing */
          }
        }}
        style={{ width: "100%", fontFamily: "monospace", fontSize: 12 }}
      />
    </div>
  );
}
