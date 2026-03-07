import { atom } from "jotai";

export interface DocMeta {
  id: string;
  title: string;
  engagementId: string;
  createdAt: number;
}

export interface Engagement {
  id: string;
  name: string;
  createdAt: number;
  docs: DocMeta[];
}

// Generate simple unique IDs
let counter = 0;
export function genId(prefix: string) {
  return `${prefix}:${Date.now()}-${++counter}`;
}

// Default engagement + doc so the workspace isn't empty on load
const defaultDocId = "doc:home";
const defaultEngagement: Engagement = {
  id: "eng:personal",
  name: "Personal",
  createdAt: Date.now(),
  docs: [
    {
      id: defaultDocId,
      title: "Untitled",
      engagementId: "eng:personal",
      createdAt: Date.now(),
    },
  ],
};

export const themeAtom = atom<"light" | "dark">("light");

export const accentColorAtom = atom<string>("#3b82f6");
export const fontSizeAtom = atom<"small" | "medium" | "large">("medium");

export const activeViewAtom = atom<"editor" | "agent-dashboard" | "pipeline-editor" | "task-queue" | "agent-triggers" | "plugin-manager" | "tools-manager" | "settings" | "timeline">("editor");

export const editorStoreAtom = atom<any>(null);

export const engagementsAtom = atom<Engagement[]>([defaultEngagement]);
export const activeDocIdAtom = atom<string>(defaultDocId);

// Tracks which docs are open as tabs
export const openDocIdsAtom = atom<string[]>([defaultDocId]);

// Write atom: open a doc as a tab and make it active
export const openDocAtom = atom(
  null,
  (get, set, docId: string) => {
    const openIds = get(openDocIdsAtom);
    if (!openIds.includes(docId)) {
      set(openDocIdsAtom, [...openIds, docId]);
    }
    set(activeDocIdAtom, docId);
  }
);

// Write atom: close a tab and switch to a neighbor if it was active
export const closeDocAtom = atom(
  null,
  (get, set, docId: string) => {
    const openIds = get(openDocIdsAtom);
    if (openIds.length <= 1) return; // don't close the last tab
    const idx = openIds.indexOf(docId);
    const next = openIds.filter((id) => id !== docId);
    set(openDocIdsAtom, next);
    if (get(activeDocIdAtom) === docId) {
      // switch to the next tab, or previous if it was last
      const newIdx = Math.min(idx, next.length - 1);
      set(activeDocIdAtom, next[newIdx]);
    }
  }
);

// Write atom: update a doc's title by docId
export const updateDocTitleAtom = atom(
  null,
  (get, set, { docId, title }: { docId: string; title: string }) => {
    const engagements = get(engagementsAtom);
    set(
      engagementsAtom,
      engagements.map((eng) => ({
        ...eng,
        docs: eng.docs.map((d) =>
          d.id === docId ? { ...d, title } : d
        ),
      }))
    );
  }
);

// Recently opened doc IDs (most recent first, max 10)
export const recentDocIdsAtom = atom<string[]>([]);
export const trackRecentDocAtom = atom(
  null,
  (get, set, docId: string) => {
    const recent = get(recentDocIdsAtom).filter((id) => id !== docId);
    set(recentDocIdsAtom, [docId, ...recent].slice(0, 10));
  }
);

// Favorites (pinned doc IDs)
export const favoritesAtom = atom<string[]>([]);
export const toggleFavoriteAtom = atom(
  null,
  (get, set, docId: string) => {
    const favorites = get(favoritesAtom);
    if (favorites.includes(docId)) {
      set(favoritesAtom, favorites.filter((id) => id !== docId));
    } else {
      set(favoritesAtom, [...favorites, docId]);
    }
  }
);

// Chat messages
export interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  timestamp: number;
}

export const chatMessagesAtom = atom<ChatMessage[]>([
  {
    id: "msg:welcome",
    role: "system",
    content:
      "Welcome to Celerex. I'm your AI co-pilot with full context of your workspace — documents, diagrams, agent outputs, and more.\n\nHow can I help?",
    timestamp: Date.now(),
  },
]);

// Derived: get the active doc's metadata
export const activeDocAtom = atom<DocMeta | undefined>((get) => {
  const engagements = get(engagementsAtom);
  const activeId = get(activeDocIdAtom);
  for (const eng of engagements) {
    const doc = eng.docs.find((d) => d.id === activeId);
    if (doc) return doc;
  }
  return undefined;
});
