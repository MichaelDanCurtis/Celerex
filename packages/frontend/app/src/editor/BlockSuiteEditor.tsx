import { useEffect, useRef, useState } from "react";
import { useSetAtom } from "jotai";
import type { TestWorkspace } from "@blocksuite/affine/store/test";
import { updateDocTitleAtom, editorStoreAtom } from "../state/atoms";

interface BlockSuiteEditorProps {
  mode: "page" | "edgeless";
  workspace: TestWorkspace;
  docId: string;
}

let initPromise: Promise<void> | null = null;

function initEffects(): Promise<void> {
  if (!initPromise) {
    initPromise = (async () => {
      await import("@blocksuite/affine/effects");
      if (!customElements.get("affine-editor-container")) {
        const { effects: itEffects } = await import("@blocksuite/integration-test/effects");
        itEffects();
      }
    })();
  }
  return initPromise;
}

export function BlockSuiteEditor({ mode, workspace, docId }: BlockSuiteEditorProps) {
  // This ref is exclusively for the web component — React never renders children into it
  const containerRef = useRef<HTMLDivElement>(null);
  const editorRef = useRef<any>(null);
  const storeRef = useRef<any>(null);
  const [ready, setReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const updateDocTitle = useSetAtom(updateDocTitleAtom);
  const setEditorStore = useSetAtom(editorStoreAtom);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    let cancelled = false;

    async function mount() {
      try {
        await initEffects();
        if (cancelled) return;

        const { getOrCreateDoc, initEmptyDoc } = await import("./setup");
        const { getTestViewManager } = await import("@blocksuite/integration-test/view");
        const { getTestStoreManager } = await import("@blocksuite/integration-test/store");

        if (cancelled) return;

        const storeManager = getTestStoreManager();
        workspace.storeExtensions = storeManager.get();

        const doc = getOrCreateDoc(workspace, docId);
        const store = doc.getStore();

        if (!store.root) {
          initEmptyDoc(store);
        }

        storeRef.current = store;
        setEditorStore(store);

        const editor = document.createElement("affine-editor-container") as any;
        editor.autofocus = true;
        editor.doc = store;
        editor.mode = mode;

        const viewManager = getTestViewManager();
        editor.pageSpecs = viewManager.get("page");
        editor.edgelessSpecs = viewManager.get("edgeless");

        editor.style.width = "100%";
        editor.style.height = "100%";

        if (cancelled) return;

        container.appendChild(editor);
        editorRef.current = editor;
        setReady(true);
        setError(null);
      } catch (err: any) {
        if (cancelled) return;
        console.error("Failed to mount BlockSuite editor:", err);
        setError(err.message || "Failed to load editor");
      }
    }

    mount();

    return () => {
      cancelled = true;
      editorRef.current = null;
      storeRef.current = null;
      setEditorStore(null);
      // Directly clean up DOM — this div is not managed by React
      while (container.firstChild) {
        container.removeChild(container.firstChild);
      }
    };
  }, [workspace, docId]);

  useEffect(() => {
    if (editorRef.current && ready) {
      editorRef.current.mode = mode;
    }
  }, [mode, ready]);

  // Sync editor title changes back to Jotai state
  useEffect(() => {
    const store = storeRef.current;
    if (!store || !ready) return;

    const root = store.root;
    if (!root) return;

    // Title is stored as a Yjs Text in the yBlock map
    const yText = root.yBlock?.get("prop:title");
    if (!yText) return;

    const syncTitle = () => {
      const newTitle = yText.toString();
      updateDocTitle({ docId, title: newTitle });
    };

    // Set initial title
    syncTitle();

    // Observe the Yjs Text object for real-time changes
    yText.observe(syncTitle);
    return () => {
      yText.unobserve(syncTitle);
    };
  }, [ready, docId, updateDocTitle]);

  return (
    <div style={{ width: "100%", height: "100%", position: "relative" }}>
      {/* Web component container — React never renders children here */}
      <div
        ref={containerRef}
        style={{
          width: "100%",
          height: "100%",
          overflow: "auto",
          display: ready ? "block" : "none",
        }}
      />
      {/* Status overlays — managed by React, separate from web component */}
      {error && (
        <div style={{ padding: 24, color: "#ef4444", fontSize: 14 }}>
          <p style={{ fontWeight: 600, marginBottom: 8 }}>Editor failed to load</p>
          <p style={{ fontSize: 12, fontFamily: "monospace" }}>{error}</p>
        </div>
      )}
      {!ready && !error && (
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            height: "100%",
            color: "var(--celerex-text-secondary)",
            fontSize: 14,
          }}
        >
          Loading editor...
        </div>
      )}
    </div>
  );
}
