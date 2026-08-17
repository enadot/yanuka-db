import { useEffect, useState } from 'react';

/**
 * Delay propagating a rapidly-changing value.
 *
 * Search runs against a local database, so the cost of an extra query is small
 * — but re-rendering a full result list on every keystroke is visibly janky,
 * and results that reshuffle mid-word are hard to read. 150ms is short enough
 * to feel instant and long enough to skip the intermediate states of a word.
 */
export function useDebouncedValue<T>(value: T, delayMs = 150): T {
  const [debounced, setDebounced] = useState(value);

  useEffect(() => {
    const timer = setTimeout(() => setDebounced(value), delayMs);
    return () => clearTimeout(timer);
  }, [value, delayMs]);

  return debounced;
}
