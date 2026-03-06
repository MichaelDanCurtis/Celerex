import { TestWorkspace } from "@blocksuite/affine/store/test";

let _workspace: TestWorkspace | null = null;

export function getWorkspace(): TestWorkspace {
  if (_workspace) return _workspace;

  _workspace = new TestWorkspace({
    id: "celerex-workspace",
  });
  _workspace.meta.initialize();
  _workspace.start();

  return _workspace;
}

export function getOrCreateDoc(workspace: TestWorkspace, docId: string) {
  let doc = workspace.getDoc(docId);
  if (!doc) {
    doc = workspace.createDoc(docId);
  }
  if (!doc.loaded) {
    doc.load();
  }
  return doc;
}

export function initEmptyDoc(store: ReturnType<ReturnType<typeof getOrCreateDoc>["getStore"]>) {
  const rootId = store.addBlock("affine:page", {});
  store.addBlock("affine:surface", {}, rootId);
  const noteId = store.addBlock("affine:note", {}, rootId);
  store.addBlock("affine:paragraph", { type: "text" }, noteId);
}
