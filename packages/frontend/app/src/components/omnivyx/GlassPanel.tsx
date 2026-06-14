import type { CSSProperties, ReactNode } from "react";

export function GlassPanel({
  children,
  style,
  testId,
}: {
  children?: ReactNode;
  style?: CSSProperties;
  testId?: string;
}) {
  return (
    <div className="ov-glass" data-testid={testId} style={{ position: "relative", padding: 14, ...style }}>
      {children}
    </div>
  );
}
