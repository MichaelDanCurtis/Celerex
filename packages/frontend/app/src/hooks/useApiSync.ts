import { useEffect, useRef, useState } from "react";
import { useAtom } from "jotai";
import { engagementsAtom, Engagement, DocMeta } from "../state/atoms";
import { fetchEngagements, ApiEngagement } from "../api/client";

/**
 * Maps an ApiEngagement (snake_case from the Rust backend) to the Jotai
 * Engagement format (camelCase used in the frontend).
 */
function mapApiEngagement(api: ApiEngagement): Engagement {
  return {
    id: api.id,
    name: api.name,
    createdAt: new Date(api.created_at).getTime(),
    docs: api.docs.map(
      (d): DocMeta => ({
        id: d.id,
        title: d.title,
        engagementId: d.engagement_id,
        createdAt: new Date(d.created_at).getTime(),
      }),
    ),
  };
}

/**
 * Syncs Jotai engagement state with the Rust backend API on startup.
 *
 * If the backend is unreachable the hook silently falls back to whatever
 * in-memory state already exists in the engagementsAtom, logging a warning
 * to the console.
 */
export function useApiSync(): { synced: boolean; error: string | null } {
  const [, setEngagements] = useAtom(engagementsAtom);
  const [synced, setSynced] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const fetchedRef = useRef(false);

  useEffect(() => {
    // Prevent double-fetching in React StrictMode (dev-only double mount).
    if (fetchedRef.current) return;
    fetchedRef.current = true;

    let cancelled = false;

    async function sync() {
      try {
        const apiEngagements = await fetchEngagements();

        if (cancelled) return;

        const mapped = apiEngagements.map(mapApiEngagement);
        setEngagements(mapped);
        setSynced(true);
      } catch (err) {
        if (cancelled) return;

        const message =
          err instanceof Error ? err.message : "Unknown error";
        console.warn(
          "[useApiSync] Failed to fetch engagements from backend — falling back to in-memory state:",
          message,
        );
        setError(message);
        // Mark as synced even on failure so consumers know the attempt completed.
        setSynced(true);
      }
    }

    sync();

    return () => {
      cancelled = true;
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  return { synced, error };
}
