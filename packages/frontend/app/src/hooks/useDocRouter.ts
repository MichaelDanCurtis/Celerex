import { useEffect } from "react";
import { useAtom, useSetAtom } from "jotai";
import { activeDocIdAtom, openDocAtom } from "../state/atoms";

const HASH_PREFIX = "#doc:";

function docIdFromHash(hash: string): string | null {
  if (hash.startsWith(HASH_PREFIX)) {
    return hash.slice(1); // strip the leading "#", keep "doc:..."
  }
  return null;
}

function hashFromDocId(docId: string): string {
  return `#${docId}`; // e.g. "#doc:home"
}

/**
 * Syncs the active document with the URL hash so every doc gets a shareable
 * URL and browser back/forward navigation works.
 *
 * Hash format: #doc:<id>  (e.g. #doc:home)
 */
export function useDocRouter() {
  const [activeDocId, setActiveDocId] = useAtom(activeDocIdAtom);
  const openDoc = useSetAtom(openDocAtom);

  // On mount: if the URL already contains a doc hash, navigate to it
  useEffect(() => {
    const docId = docIdFromHash(window.location.hash);
    if (docId) {
      openDoc(docId);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // When the active doc changes, update the URL hash (without adding a
  // history entry — we use replaceState so normal in-app navigation doesn't
  // pollute the history stack).
  useEffect(() => {
    const desired = hashFromDocId(activeDocId);
    if (window.location.hash !== desired) {
      history.replaceState(null, "", desired);
    }
  }, [activeDocId]);

  // Listen for popstate (browser back / forward) and sync the active doc.
  useEffect(() => {
    function onPopState() {
      const docId = docIdFromHash(window.location.hash);
      if (docId) {
        openDoc(docId);
      }
    }

    window.addEventListener("popstate", onPopState);
    return () => window.removeEventListener("popstate", onPopState);
  }, [openDoc]);
}
