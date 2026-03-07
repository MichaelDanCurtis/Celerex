import { atom } from "jotai";

export interface ActivityEntry {
  id: string;
  action: string;
  target: string;
  timestamp: number;
  user: string;
}

const now = Date.now();

export const activityLogAtom = atom<ActivityEntry[]>([
  {
    id: "act:seed-1",
    action: "Created document",
    target: "Project Brief",
    timestamp: now - 1000 * 60 * 3,
    user: "Alice",
  },
  {
    id: "act:seed-2",
    action: "Edited",
    target: "Meeting Notes",
    timestamp: now - 1000 * 60 * 12,
    user: "Bob",
  },
  {
    id: "act:seed-3",
    action: "Shared with team",
    target: "Q4 Report",
    timestamp: now - 1000 * 60 * 45,
    user: "Alice",
  },
  {
    id: "act:seed-4",
    action: "Tagged as Final",
    target: "API Spec",
    timestamp: now - 1000 * 60 * 60 * 2,
    user: "Carol",
  },
  {
    id: "act:seed-5",
    action: "Deleted",
    target: "Old Draft",
    timestamp: now - 1000 * 60 * 60 * 5,
    user: "Bob",
  },
  {
    id: "act:seed-6",
    action: "Commented on",
    target: "Design System",
    timestamp: now - 1000 * 60 * 60 * 8,
    user: "Alice",
  },
  {
    id: "act:seed-7",
    action: "Renamed",
    target: "Sprint Retro",
    timestamp: now - 1000 * 60 * 60 * 24,
    user: "Carol",
  },
  {
    id: "act:seed-8",
    action: "Archived",
    target: "2024 Budget",
    timestamp: now - 1000 * 60 * 60 * 48,
    user: "Bob",
  },
  {
    id: "act:seed-9",
    action: "Restored",
    target: "Onboarding Guide",
    timestamp: now - 1000 * 60 * 60 * 72,
    user: "Alice",
  },
  {
    id: "act:seed-10",
    action: "Created document",
    target: "Roadmap v2",
    timestamp: now - 1000 * 60 * 60 * 120,
    user: "Carol",
  },
]);

let activityCounter = 0;

export const addActivityAtom = atom(
  null,
  (get, set, entry: Omit<ActivityEntry, "id" | "timestamp">) => {
    const id = `act:${Date.now()}-${++activityCounter}`;
    const newEntry: ActivityEntry = {
      ...entry,
      id,
      timestamp: Date.now(),
    };
    set(activityLogAtom, [newEntry, ...get(activityLogAtom)]);
  }
);
