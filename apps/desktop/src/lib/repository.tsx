import { createContext, useContext, useMemo, type ReactNode } from 'react';
import { MockRepository, type ContactsRepository } from '@yanuka/core';
import { TauriRepository, isTauri } from './tauri-repository';

const RepositoryContext = createContext<ContactsRepository | null>(null);

/**
 * Choose the data source for this run.
 *
 * Inside a Tauri window the real SQLite database is used. In a plain browser
 * — development on a machine without the Tauri toolchain, CI, design review —
 * the in-memory repository takes over, running the same search engine over the
 * demo dataset. `VITE_DATA_SOURCE=mock` forces the latter even inside Tauri,
 * which is useful for reproducing a UI bug against known data.
 */
function createRepository(): ContactsRepository {
  const forced = import.meta.env.VITE_DATA_SOURCE;
  if (forced === 'mock') return new MockRepository(undefined, 40);
  if (forced === 'tauri') return new TauriRepository();
  return isTauri() ? new TauriRepository() : new MockRepository(undefined, 40);
}

export function RepositoryProvider({ children }: { children: ReactNode }) {
  const repository = useMemo(() => createRepository(), []);
  return <RepositoryContext.Provider value={repository}>{children}</RepositoryContext.Provider>;
}

export function useRepository(): ContactsRepository {
  const repository = useContext(RepositoryContext);
  if (!repository) {
    throw new Error('useRepository must be used inside a RepositoryProvider');
  }
  return repository;
}

/** Whether this run is backed by the real local database. */
export function useIsLocalDatabase(): boolean {
  return isTauri() && import.meta.env.VITE_DATA_SOURCE !== 'mock';
}
