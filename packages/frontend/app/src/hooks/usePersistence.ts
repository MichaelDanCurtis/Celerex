import { useState, useEffect, useRef, useCallback } from "react";
import { useAtom } from "jotai";
import { engagementsAtom, activeDocIdAtom, themeAtom, favoritesAtom } from "../state/atoms";
import {
  saveEngagements,
  loadEngagements,
  saveSetting,
  loadSetting,
} from "../persistence/storage";

export function usePersistence(): { loaded: boolean } {
  const [loaded, setLoaded] = useState(false);
  const [engagements, setEngagements] = useAtom(engagementsAtom);
  const [activeDocId, setActiveDocId] = useAtom(activeDocIdAtom);
  const [theme, setTheme] = useAtom(themeAtom);
  const [favorites, setFavorites] = useAtom(favoritesAtom);

  // Track whether initial load has completed so we don't persist defaults
  const initialLoadDone = useRef(false);

  // --- Debounce timers ---
  const engagementsTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const activeDocIdTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const themeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const favoritesTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // --- Load from IndexedDB on mount ---
  useEffect(() => {
    let cancelled = false;

    async function hydrate() {
      const [savedEngagements, savedActiveDocId, savedTheme, savedFavorites] =
        await Promise.all([
          loadEngagements(),
          loadSetting("activeDocId"),
          loadSetting("theme"),
          loadSetting("favorites"),
        ]);

      if (cancelled) return;

      if (savedEngagements) {
        setEngagements(savedEngagements);
      }
      if (savedActiveDocId) {
        setActiveDocId(savedActiveDocId);
      }
      if (savedTheme) {
        setTheme(savedTheme);
      }
      if (savedFavorites) {
        setFavorites(savedFavorites);
      }

      initialLoadDone.current = true;
      setLoaded(true);
    }

    hydrate();

    return () => {
      cancelled = true;
    };
    // Run only on mount
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // --- Debounced save helpers ---
  const debouncedSaveEngagements = useCallback(
    (value: typeof engagements) => {
      if (engagementsTimer.current) clearTimeout(engagementsTimer.current);
      engagementsTimer.current = setTimeout(() => {
        saveEngagements(value);
      }, 300);
    },
    [],
  );

  const debouncedSaveSetting = useCallback(
    (key: string, value: any, timerRef: React.MutableRefObject<ReturnType<typeof setTimeout> | null>) => {
      if (timerRef.current) clearTimeout(timerRef.current);
      timerRef.current = setTimeout(() => {
        saveSetting(key, value);
      }, 300);
    },
    [],
  );

  // --- Persist engagements on change ---
  useEffect(() => {
    if (!initialLoadDone.current) return;
    debouncedSaveEngagements(engagements);
  }, [engagements, debouncedSaveEngagements]);

  // --- Persist activeDocId on change ---
  useEffect(() => {
    if (!initialLoadDone.current) return;
    debouncedSaveSetting("activeDocId", activeDocId, activeDocIdTimer);
  }, [activeDocId, debouncedSaveSetting]);

  // --- Persist theme on change ---
  useEffect(() => {
    if (!initialLoadDone.current) return;
    debouncedSaveSetting("theme", theme, themeTimer);
  }, [theme, debouncedSaveSetting]);

  // --- Persist favorites on change ---
  useEffect(() => {
    if (!initialLoadDone.current) return;
    debouncedSaveSetting("favorites", favorites, favoritesTimer);
  }, [favorites, debouncedSaveSetting]);

  // --- Cleanup timers on unmount ---
  useEffect(() => {
    return () => {
      if (engagementsTimer.current) clearTimeout(engagementsTimer.current);
      if (activeDocIdTimer.current) clearTimeout(activeDocIdTimer.current);
      if (themeTimer.current) clearTimeout(themeTimer.current);
      if (favoritesTimer.current) clearTimeout(favoritesTimer.current);
    };
  }, []);

  return { loaded };
}
