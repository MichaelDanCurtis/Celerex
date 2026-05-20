import { useCallback, useEffect, useRef, useState } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import {
  projectsAtom,
  tokenAtom,
  type Project,
} from "../state/atoms";
import { listProjects, type ApiProject } from "../api/client";

function mapApiProject(api: ApiProject): Project {
  return {
    id: api.id,
    name: api.name,
    engagementId: api.engagement_id,
  };
}

/**
 * Pulls the list of projects from the backend (filtered by the user's JWT
 * claims) and mirrors them into `projectsAtom`. Exposes a `refresh()` callback
 * for callers that need to re-sync after a mutation (create/delete).
 *
 * Silently falls back to whatever is already in `projectsAtom` if the backend
 * is unreachable or the token is missing.
 */
export function useProjectsSync(): {
  synced: boolean;
  error: string | null;
  refresh: () => Promise<void>;
} {
  const token = useAtomValue(tokenAtom);
  const setProjects = useSetAtom(projectsAtom);
  const [synced, setSynced] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const fetchedRef = useRef(false);

  const refresh = useCallback(async () => {
    if (!token) {
      setSynced(true);
      return;
    }
    try {
      const apiProjects = await listProjects(token);
      setProjects(apiProjects.map(mapApiProject));
      setSynced(true);
      setError(null);
    } catch (err) {
      const message = err instanceof Error ? err.message : "Unknown error";
      console.warn(
        "[useProjectsSync] Failed to fetch projects — falling back to in-memory state:",
        message,
      );
      setError(message);
      setSynced(true);
    }
  }, [token, setProjects]);

  useEffect(() => {
    if (fetchedRef.current) return;
    fetchedRef.current = true;
    void refresh();
  }, [refresh]);

  return { synced, error, refresh };
}
