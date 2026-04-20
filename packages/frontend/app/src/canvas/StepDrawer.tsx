export function StepDrawer({
  stepId,
  inputs,
  outputs,
  error,
  onClose,
}: {
  stepId: string;
  inputs?: unknown;
  outputs?: unknown;
  error?: string;
  onClose: () => void;
}) {
  return (
    <div
      data-testid="step-drawer"
      style={{
        position: "absolute",
        right: 0,
        top: 0,
        width: 360,
        height: "100%",
        background: "var(--celerex-bg-primary)",
        borderLeft: "1px solid var(--celerex-border)",
        padding: 16,
        overflow: "auto",
      }}
    >
      <button onClick={onClose} aria-label="Close">
        ✕
      </button>
      <h3>{stepId}</h3>
      {error && <pre style={{ color: "#dc2626" }}>{error}</pre>}
      <h4>Inputs</h4>
      <pre>{JSON.stringify(inputs, null, 2)}</pre>
      <h4>Outputs</h4>
      <pre>{JSON.stringify(outputs, null, 2)}</pre>
    </div>
  );
}
