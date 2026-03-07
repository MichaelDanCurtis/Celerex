import { atom } from "jotai";

export interface Reaction {
  emoji: string;
  count: number;
  userReacted: boolean;
}

export const docReactionsAtom = atom<Record<string, Reaction[]>>({
  "doc:home": [
    { emoji: "\u{1F44D}", count: 3, userReacted: false },
    { emoji: "\u{1F525}", count: 1, userReacted: true },
    { emoji: "\u{1F4A1}", count: 2, userReacted: false },
  ],
});

export const toggleReactionAtom = atom(
  null,
  (get, set, { docId, emoji }: { docId: string; emoji: string }) => {
    const allReactions = get(docReactionsAtom);
    const current = allReactions[docId] ?? [];
    const idx = current.findIndex((r) => r.emoji === emoji);

    let updated: Reaction[];
    if (idx === -1) {
      updated = [...current, { emoji, count: 1, userReacted: true }];
    } else {
      const reaction = current[idx];
      if (reaction.userReacted) {
        const newCount = reaction.count - 1;
        if (newCount <= 0) {
          updated = current.filter((_, i) => i !== idx);
        } else {
          updated = current.map((r, i) =>
            i === idx ? { ...r, count: newCount, userReacted: false } : r
          );
        }
      } else {
        updated = current.map((r, i) =>
          i === idx ? { ...r, count: r.count + 1, userReacted: true } : r
        );
      }
    }

    set(docReactionsAtom, { ...allReactions, [docId]: updated });
  }
);
