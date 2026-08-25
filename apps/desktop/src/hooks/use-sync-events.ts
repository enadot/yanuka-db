import { useEffect } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { onSyncChanged } from '../lib/desktop-io';

/**
 * Keep the screen honest about what the background loop brought in.
 *
 * Without this, a contact edited on the phone arrives in the local database and
 * the desktop keeps showing the old one until something happens to refetch —
 * which reads, correctly, as sync not working. Every query is invalidated
 * rather than a chosen few: a single arriving change can touch a contact, a
 * search result, the counts on the settings screen and the conflict list at
 * once, and enumerating that set here would be a second copy of the schema
 * kept in step by hand.
 */
export function useSyncEvents() {
  const queryClient = useQueryClient();

  useEffect(() => {
    let stop: (() => void) | undefined;
    let cancelled = false;

    void onSyncChanged(() => {
      void queryClient.invalidateQueries();
    }).then((unsubscribe) => {
      // The listener resolves asynchronously, so an unmount can beat it here.
      if (cancelled) unsubscribe();
      else stop = unsubscribe;
    });

    return () => {
      cancelled = true;
      stop?.();
    };
  }, [queryClient]);
}
