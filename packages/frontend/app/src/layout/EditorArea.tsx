import { useMemo } from "react";
import { BlockSuiteEditor } from "../editor/BlockSuiteEditor";
import { getWorkspace } from "../editor/setup";

interface EditorAreaProps {
  mode: "page" | "edgeless";
}

export function EditorArea({ mode }: EditorAreaProps) {
  const workspace = useMemo(() => getWorkspace(), []);

  return (
    <div
      style={{
        flex: 1,
        height: "100%",
        overflow: "hidden",
        background: mode === "edgeless" ? "#f8f9fa" : "var(--celerex-bg-primary)",
      }}
    >
      <BlockSuiteEditor
        mode={mode}
        workspace={workspace}
        docId="doc:home"
      />
    </div>
  );
}
