import type { ReactNode } from "react";

export function GlassButton({
  children,
  onClick,
  prominent = false,
  testId,
}: {
  children: ReactNode;
  onClick?: () => void;
  prominent?: boolean;
  testId?: string;
}) {
  return (
    <button
      type="button"
      data-testid={testId}
      onClick={onClick}
      className="ov-glass"
      style={{
        padding: "8px 16px",
        borderRadius: "var(--ov-radius)",
        font: "inherit",
        fontSize: 13,
        fontWeight: 500,
        color: "var(--ov-text)",
        cursor: "pointer",
        background: prominent ? "var(--ov-glass-tint)" : "var(--ov-glass-fill)",
      }}
    >
      {children}
    </button>
  );
}
