import { QueryClient } from '@tanstack/react-query';

/**
 * The application's one QueryClient.
 *
 * The data source is a local database reached over Tauri IPC — or the
 * in-memory repository in a browser — never the network. TanStack Query does
 * not know that: its default `networkMode: 'online'` pauses every query and
 * every mutation the moment the window reports it is offline, and resumes them
 * when it reports online again. For an archive whose first promise is to work
 * with no network, that turned "save" into "save once the Wi-Fi is back"
 * (ADR-041). `'always'` on both, so connectivity never enters the picture.
 *
 * A factory rather than a module-level instance, so a test can build the same
 * client without mounting the application.
 */
export function createQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        networkMode: 'always',
        // Refetching on window focus would be pure waste against a local
        // database, and retrying a failed local query just delays showing
        // the user a real error.
        refetchOnWindowFocus: false,
        retry: false,
        staleTime: 30_000,
      },
      mutations: {
        networkMode: 'always',
        retry: false,
      },
    },
  });
}
