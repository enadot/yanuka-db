import { useEffect } from 'react';

/**
 * Bind a Ctrl/Cmd shortcut.
 *
 * Matching is done on `event.code`, not `event.key`. With a Hebrew keyboard
 * layout active — the normal case for this application — pressing the physical
 * K key reports `event.key === 'ל'`, so a `key`-based binding silently stops
 * working for exactly the users it was built for. `code` describes the physical
 * key and is layout-independent.
 *
 * @param code physical key code, e.g. `KeyK`
 */
export function useCommandHotkey(code: string, handler: () => void): void {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey)) return;
      if (event.code !== code) return;
      event.preventDefault();
      handler();
    };

    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [code, handler]);
}
