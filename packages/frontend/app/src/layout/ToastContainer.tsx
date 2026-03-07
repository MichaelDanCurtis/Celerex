import { useEffect, useState } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { toastsAtom, removeToastAtom, type Toast } from "../state/toastAtom";

const borderColors: Record<Toast["type"], string> = {
  info: "#3b82f6",
  success: "#22c55e",
  error: "#ef4444",
};

function ToastItem({ toast }: { toast: Toast }) {
  const removeToast = useSetAtom(removeToastAtom);
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    // Trigger enter animation on next frame
    const raf = requestAnimationFrame(() => setVisible(true));
    return () => cancelAnimationFrame(raf);
  }, []);

  useEffect(() => {
    if (!toast.duration) return;
    const fadeTimer = setTimeout(() => setVisible(false), toast.duration);
    const removeTimer = setTimeout(
      () => removeToast(toast.id),
      toast.duration + 300
    );
    return () => {
      clearTimeout(fadeTimer);
      clearTimeout(removeTimer);
    };
  }, [toast.id, toast.duration, removeToast]);

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "10px 14px",
        borderRadius: 8,
        borderLeft: `4px solid ${borderColors[toast.type]}`,
        background: "var(--celerex-bg-primary, #fff)",
        color: "var(--celerex-text-primary, #111)",
        boxShadow: "0 4px 12px rgba(0,0,0,0.15)",
        fontSize: 13,
        minWidth: 220,
        maxWidth: 360,
        opacity: visible ? 1 : 0,
        transform: visible ? "translateX(0)" : "translateX(20px)",
        transition: "opacity 0.3s ease, transform 0.3s ease",
        pointerEvents: "auto",
      }}
    >
      <span style={{ flex: 1 }}>{toast.message}</span>
      <span
        onClick={() => {
          setVisible(false);
          setTimeout(() => removeToast(toast.id), 300);
        }}
        style={{
          cursor: "pointer",
          opacity: 0.5,
          fontSize: 16,
          lineHeight: 1,
          flexShrink: 0,
          userSelect: "none",
        }}
        title="Dismiss"
      >
        &times;
      </span>
    </div>
  );
}

export function ToastContainer() {
  const toasts = useAtomValue(toastsAtom);

  if (toasts.length === 0) return null;

  return (
    <div
      style={{
        position: "fixed",
        bottom: 16,
        right: 16,
        zIndex: 9999,
        display: "flex",
        flexDirection: "column",
        gap: 8,
        pointerEvents: "none",
      }}
    >
      {toasts.map((toast) => (
        <ToastItem key={toast.id} toast={toast} />
      ))}
    </div>
  );
}
