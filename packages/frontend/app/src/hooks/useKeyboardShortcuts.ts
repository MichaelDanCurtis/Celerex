import { useEffect } from "react";

export interface KeyboardShortcutCallbacks {
  onNewDoc?: () => void;
  onToggleAI?: () => void;
  onToggleTheme?: () => void;
}

export function useKeyboardShortcuts(callbacks: KeyboardShortcutCallbacks) {
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      const mod = e.metaKey || e.ctrlKey;

      // Cmd/Ctrl+N — create new doc
      if (mod && e.key === "n" && !e.shiftKey) {
        e.preventDefault();
        callbacks.onNewDoc?.();
        return;
      }

      // Cmd/Ctrl+\ — toggle AI panel
      if (mod && e.key === "\\" && !e.shiftKey) {
        e.preventDefault();
        callbacks.onToggleAI?.();
        return;
      }

      // Cmd/Ctrl+Shift+L — toggle theme
      if (mod && e.shiftKey && e.key === "L") {
        e.preventDefault();
        callbacks.onToggleTheme?.();
        return;
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [callbacks]);
}
