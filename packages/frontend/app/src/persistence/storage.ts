import type { Engagement } from "../state/atoms";

const DB_NAME = "celerex";
const DB_VERSION = 1;

function openDB(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);

    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains("engagements")) {
        db.createObjectStore("engagements", { keyPath: "id" });
      }
      if (!db.objectStoreNames.contains("settings")) {
        db.createObjectStore("settings", { keyPath: "key" });
      }
    };

    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

export async function saveEngagements(engagements: Engagement[]): Promise<void> {
  try {
    const db = await openDB();
    const tx = db.transaction("engagements", "readwrite");
    const store = tx.objectStore("engagements");
    store.clear();
    for (const eng of engagements) {
      store.put(eng);
    }
    await new Promise<void>((resolve, reject) => {
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
    });
    db.close();
  } catch (err) {
    console.warn("[celerex] Failed to save engagements:", err);
  }
}

export async function loadEngagements(): Promise<Engagement[] | null> {
  try {
    const db = await openDB();
    const tx = db.transaction("engagements", "readonly");
    const store = tx.objectStore("engagements");
    const request = store.getAll();
    const result = await new Promise<Engagement[]>((resolve, reject) => {
      request.onsuccess = () => resolve(request.result as Engagement[]);
      request.onerror = () => reject(request.error);
    });
    db.close();
    return result.length > 0 ? result : null;
  } catch (err) {
    console.warn("[celerex] Failed to load engagements:", err);
    return null;
  }
}

export async function saveSetting(key: string, value: any): Promise<void> {
  try {
    const db = await openDB();
    const tx = db.transaction("settings", "readwrite");
    const store = tx.objectStore("settings");
    store.put({ key, value });
    await new Promise<void>((resolve, reject) => {
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
    });
    db.close();
  } catch (err) {
    console.warn("[celerex] Failed to save setting:", key, err);
  }
}

export async function loadSetting(key: string): Promise<any> {
  try {
    const db = await openDB();
    const tx = db.transaction("settings", "readonly");
    const store = tx.objectStore("settings");
    const request = store.get(key);
    const result = await new Promise<any>((resolve, reject) => {
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
    db.close();
    return result?.value;
  } catch (err) {
    console.warn("[celerex] Failed to load setting:", key, err);
    return undefined;
  }
}
