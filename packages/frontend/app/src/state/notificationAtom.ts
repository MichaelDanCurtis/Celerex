import { atom } from "jotai";

export interface Notification {
  id: string;
  title: string;
  message: string;
  type: "info" | "success" | "warning" | "error";
  read: boolean;
  timestamp: number;
}

const now = Date.now();

export const notificationsAtom = atom<Notification[]>([
  {
    id: "notif:seed-1",
    title: "Welcome to Celerex",
    message: "Your workspace is ready. Create a new document to get started.",
    type: "info",
    read: false,
    timestamp: now - 60000,
  },
  {
    id: "notif:seed-2",
    title: "Editor loaded",
    message: "BlockSuite editor initialized successfully.",
    type: "success",
    read: false,
    timestamp: now - 30000,
  },
  {
    id: "notif:seed-3",
    title: "Backend offline",
    message: "Connect the Rust backend for AI features and real-time sync.",
    type: "warning",
    read: false,
    timestamp: now,
  },
]);

export const unreadCountAtom = atom((get) =>
  get(notificationsAtom).filter((n) => !n.read).length
);

export const markReadAtom = atom(null, (get, set, id: string) => {
  set(
    notificationsAtom,
    get(notificationsAtom).map((n) =>
      n.id === id ? { ...n, read: true } : n
    )
  );
});

export const markAllReadAtom = atom(null, (get, set) => {
  set(
    notificationsAtom,
    get(notificationsAtom).map((n) => ({ ...n, read: true }))
  );
});

let notifCounter = 0;

export const addNotificationAtom = atom(
  null,
  (get, set, notif: Omit<Notification, "id" | "read" | "timestamp">) => {
    const id = `notif:${Date.now()}-${++notifCounter}`;
    const newNotif: Notification = {
      ...notif,
      id,
      read: false,
      timestamp: Date.now(),
    };
    set(notificationsAtom, [newNotif, ...get(notificationsAtom)]);
  }
);
