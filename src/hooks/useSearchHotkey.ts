import { useEffect, useRef } from "react";

const SEARCH_COMMIT_DELAY_MS = 100;

export function useSearchHotkey(
  openSearch: (() => void) | undefined,
  searchValue: string,
  commitSearch: (value: string) => void,
) {
  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const commitTimerRef = useRef<number | null>(null);
  const openSearchRef = useRef(openSearch);
  const commitSearchRef = useRef(commitSearch);
  openSearchRef.current = openSearch;
  commitSearchRef.current = commitSearch;

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        const open = openSearchRef.current;
        if (!open || !searchInputRef.current) {
          return;
        }
        event.preventDefault();
        open();
        window.requestAnimationFrame(() => {
          searchInputRef.current?.focus();
          searchInputRef.current?.select();
        });
      }
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  useEffect(() => {
    const input = searchInputRef.current;
    if (commitTimerRef.current === null && input && input.value !== searchValue) {
      input.value = searchValue;
    }
  }, [searchValue]);

  useEffect(
    () => () => {
      if (commitTimerRef.current !== null) {
        window.clearTimeout(commitTimerRef.current);
      }
    },
    [],
  );

  function queueSearch(value: string) {
    if (commitTimerRef.current !== null) {
      window.clearTimeout(commitTimerRef.current);
    }
    commitTimerRef.current = window.setTimeout(() => {
      commitTimerRef.current = null;
      commitSearchRef.current(value);
    }, SEARCH_COMMIT_DELAY_MS);
  }

  function cancelQueuedSearch() {
    if (commitTimerRef.current !== null) {
      window.clearTimeout(commitTimerRef.current);
      commitTimerRef.current = null;
    }
  }

  function flushSearch(value: string) {
    cancelQueuedSearch();
    if (value !== searchValue) {
      commitSearchRef.current(value);
    }
  }

  return { searchInputRef, queueSearch, cancelQueuedSearch, flushSearch };
}
