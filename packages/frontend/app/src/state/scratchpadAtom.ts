import { atom } from "jotai";

export const scratchpadAtom = atom<string>(
  "Quick notes...\n\n- Use this scratchpad for quick thoughts\n- Notes persist during your session"
);
