import { useEffect, useRef } from "react";

export function useSearchHotkey(openSearch?: () => void) {
  const searchInputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        if (!searchInputRef.current) {
          return;
        }
        event.preventDefault();
        openSearch?.();
        window.requestAnimationFrame(() => {
          searchInputRef.current?.focus();
          searchInputRef.current?.select();
        });
      }
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [openSearch]);

  return searchInputRef;
}
