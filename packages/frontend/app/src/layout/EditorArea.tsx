import { useMemo } from "react";
import { useAtomValue } from "jotai";
import { BlockSuiteEditor } from "../editor/BlockSuiteEditor";
import { getWorkspace } from "../editor/setup";
import { activeDocIdAtom } from "../state/atoms";
import { TabBar } from "./TabBar";
import { StatusBar } from "./StatusBar";

interface EditorAreaProps {
  mode: "page" | "edgeless";
  activeView?: string;
}

export function EditorArea({ mode, activeView }: EditorAreaProps) {
  const workspace = useMemo(() => getWorkspace(), []);
  const activeDocId = useAtomValue(activeDocIdAtom);

  if (activeView && activeView !== "editor") {
    return null;
  }

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
        display: "flex",
        flexDirection: "column",
        background:
          mode === "edgeless" ? "#f8f9fa" : "var(--celerex-bg-primary)",
      }}
    >
      <TabBar />
      <div style={{ flex: 1, overflow: "hidden" }}>
        <BlockSuiteEditor mode={mode} workspace={workspace} docId={activeDocId} />
      </div>
      <StatusBar mode={mode} />
    </div>
  );
}
