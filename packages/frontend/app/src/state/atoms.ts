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

export const editorStoreAtom = atom<any>(null);

export const engagementsAtom = atom<Engagement[]>([defaultEngagement]);
export const activeDocIdAtom = atom<string>(defaultDocId);

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
