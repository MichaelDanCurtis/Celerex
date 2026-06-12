import { atom } from "jotai";
import { engagementsAtom } from "./atoms";

export interface TrashedDoc {
  id: string;
  title: string;
  engagementName: string;
  deletedAt: number;
}

export const trashAtom = atom<TrashedDoc[]>([
  { id: "doc:trash-1", title: "Old Draft", engagementName: "Personal", deletedAt: Date.now() - 86400000 },
  { id: "doc:trash-2", title: "Meeting Notes (Apr)", engagementName: "Personal", deletedAt: Date.now() - 172800000 },
  { id: "doc:trash-3", title: "Deprecated Spec", engagementName: "Personal", deletedAt: Date.now() - 604800000 },
]);

// Move a doc from engagements into trash
export const moveToTrashAtom = atom(null, (get, set, docId: string) => {
  const engagements = get(engagementsAtom);
  let foundTitle = "";
  let foundEngName = "";

  for (const eng of engagements) {
    const doc = eng.docs.find((d) => d.id === docId);
    if (doc) {
      foundTitle = doc.title;
      foundEngName = eng.name;
      break;
    }
  }

  if (!foundTitle) return;

  // Remove from engagements
  set(
    engagementsAtom,
    engagements.map((eng) => ({
      ...eng,
      docs: eng.docs.filter((d) => d.id !== docId),
    }))
  );

  // Add to trash
  set(trashAtom, (prev) => [
    ...prev,
    { id: docId, title: foundTitle, engagementName: foundEngName, deletedAt: Date.now() },
  ]);
});

// Restore a doc from trash back to the first engagement
export const restoreFromTrashAtom = atom(null, (get, set, docId: string) => {
  const trash = get(trashAtom);
  const item = trash.find((t) => t.id === docId);
  if (!item) return;

  // Remove from trash
  set(trashAtom, trash.filter((t) => t.id !== docId));

  // Add back to the first engagement
  const engagements = get(engagementsAtom);
  if (engagements.length === 0) return;
  const firstEng = engagements[0];

  set(
    engagementsAtom,
    engagements.map((eng) =>
      eng.id === firstEng.id
        ? {
            ...eng,
            docs: [
              ...eng.docs,
              { id: docId, title: item.title, engagementId: firstEng.id, createdAt: Date.now() },
            ],
          }
        : eng
    )
  );
});

// Permanently delete a doc from trash
export const permanentDeleteAtom = atom(null, (_get, set, docId: string) => {
  set(trashAtom, (prev) => prev.filter((t) => t.id !== docId));
});
