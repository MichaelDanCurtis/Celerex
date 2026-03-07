import { atom } from "jotai";

export interface Toast {
  id: string;
  message: string;
  type: "info" | "success" | "error";
  duration?: number;
}

export const toastsAtom = atom<Toast[]>([]);

let toastCounter = 0;

export const addToastAtom = atom(
  null,
  (get, set, toast: Omit<Toast, "id">) => {
    const id = `toast:${Date.now()}-${++toastCounter}`;
    const newToast: Toast = {
      id,
      message: toast.message,
      type: toast.type,
      duration: toast.duration ?? 3000,
    };
    set(toastsAtom, [...get(toastsAtom), newToast]);
  }
);

export const removeToastAtom = atom(null, (get, set, id: string) => {
  set(
    toastsAtom,
    get(toastsAtom).filter((t) => t.id !== id)
  );
});
