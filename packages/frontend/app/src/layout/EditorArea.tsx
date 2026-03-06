import { useMemo } from "react";
import { useAtomValue } from "jotai";
import { BlockSuiteEditor } from "../editor/BlockSuiteEditor";
import { getWorkspace } from "../editor/setup";
import { activeDocIdAtom } from "../state/atoms";

interface EditorAreaProps {
  mode: "page" | "edgeless";
}

export function EditorArea({ mode }: EditorAreaProps) {
  const workspace = useMemo(() => getWorkspace(), []);
  const activeDocId = useAtomValue(activeDocIdAtom);

  if (!activeDocId) {
    return (
      <div
        style={{
          flex: 1,
          height: "100%",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          color: "var(--celerex-text-secondary)",
          fontSize: 14,
        }}
      >
        No document selected
      </div>
    );
  }

  return (
    <div
      style={{
        flex: 1,
        height: "100%",
        overflow: "hidden",
        background:
          mode === "edgeless" ? "#f8f9fa" : "var(--celerex-bg-primary)",
      }}
    >
      <BlockSuiteEditor mode={mode} workspace={workspace} docId={activeDocId} />
    </div>
  );
}
