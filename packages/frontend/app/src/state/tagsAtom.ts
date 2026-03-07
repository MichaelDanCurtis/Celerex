import { atom } from "jotai";

export interface Tag {
  id: string;
  name: string;
  color: string;
}

export const tagsAtom = atom<Tag[]>([
  { id: "tag:draft", name: "Draft", color: "#3b82f6" },
  { id: "tag:review", name: "Review", color: "#f59e0b" },
  { id: "tag:final", name: "Final", color: "#22c55e" },
  { id: "tag:archive", name: "Archive", color: "#6b7280" },
]);

export const docTagsAtom = atom<Record<string, string[]>>({});

export const toggleDocTagAtom = atom(
  null,
  (get, set, { docId, tagId }: { docId: string; tagId: string }) => {
    const docTags = get(docTagsAtom);
    const current = docTags[docId] ?? [];
    const hasTag = current.includes(tagId);
    set(docTagsAtom, {
      ...docTags,
      [docId]: hasTag
        ? current.filter((id) => id !== tagId)
        : [...current, tagId],
    });
  }
);
