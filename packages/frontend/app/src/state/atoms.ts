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

export const engagementsAtom = atom<Engagement[]>([defaultEngagement]);
export const activeDocIdAtom = atom<string>(defaultDocId);

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
