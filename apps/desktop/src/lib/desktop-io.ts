import { invoke } from '@tauri-apps/api/core';
import { buildContactsCsv, type ContactsRepository } from '@yanuka/core';
import type { ContactWithRelations } from '@yanuka/types';
import { isTauri } from './tauri-repository';

/**
 * Backup and export plumbing — the only module that talks to the desktop's
 * file commands. Everything here degrades in the browser: backup is a
 * desktop-only concept (the in-memory repository has nothing durable to back
 * up), while CSV export works everywhere via a Blob download.
 */

export interface BackupStatus {
  lastBackupAt: string | null;
  backupsDirectory: string | null;
}

export async function backupStatus(): Promise<BackupStatus | null> {
  if (!isTauri()) {
    return null;
  }
  return (await invoke('backup_status')) as BackupStatus;
}

/** Ask for a destination and snapshot the live database there. */
export async function backupNow(): Promise<string | null> {
  const { save } = await import('@tauri-apps/plugin-dialog');
  const today = new Date().toISOString().slice(0, 10);
  const target = await save({
    title: 'גיבוי המאגר',
    defaultPath: `contacts-backup-${today}.db`,
    filters: [{ name: 'SQLite', extensions: ['db'] }],
  });
  if (!target) {
    return null;
  }
  return (await invoke('backup_database', { targetPath: target })) as string;
}

/**
 * Fetch every live contact in full, for export.
 *
 * Paged through the repository like any screen would — export must never
 * bypass the data boundary, or the browser build and the desktop would export
 * different things.
 */
export async function fetchAllContacts(
  repository: ContactsRepository,
  onProgress?: (done: number, total: number) => void,
): Promise<ContactWithRelations[]> {
  const ids: string[] = [];
  let cursor: string | null = null;
  for (;;) {
    const page = await repository.listContacts({
      cursor,
      limit: 200,
      sort: 'name',
      startsWith: null,
      favoritesOnly: false,
      includeDeleted: false,
    });
    ids.push(...page.items.map((item) => item.id));
    if (!page.nextCursor) {
      break;
    }
    cursor = page.nextCursor;
  }

  const contacts: ContactWithRelations[] = [];
  for (const id of ids) {
    const contact = await repository.getContact(id);
    if (contact && contact.deletedAt == null) {
      contacts.push(contact);
    }
    onProgress?.(contacts.length, ids.length);
  }
  return contacts;
}

/** Build the CSV and hand it to the user — a save dialog or a download. */
export async function exportContactsCsv(
  repository: ContactsRepository,
  onProgress?: (done: number, total: number) => void,
): Promise<string | null> {
  const contacts = await fetchAllContacts(repository, onProgress);
  const csv = buildContactsCsv(contacts);
  const today = new Date().toISOString().slice(0, 10);
  const fileName = `contacts-${today}.csv`;

  if (isTauri()) {
    const { save } = await import('@tauri-apps/plugin-dialog');
    const target = await save({
      title: 'ייצוא אנשי קשר',
      defaultPath: fileName,
      filters: [{ name: 'CSV', extensions: ['csv'] }],
    });
    if (!target) {
      return null;
    }
    return (await invoke('save_exported_csv', { path: target, contents: csv })) as string;
  }

  const url = URL.createObjectURL(new Blob([csv], { type: 'text/csv;charset=utf-8' }));
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = fileName;
  anchor.click();
  URL.revokeObjectURL(url);
  return fileName;
}

/**
 * Sync, from the frontend's side.
 *
 * Desktop only, and honest about it: the browser build has an in-memory
 * repository with nothing durable to sync, so these report "not connected"
 * rather than pretending or throwing. That keeps the settings screen renderable
 * in the demo build, which is where its layout is actually reviewed.
 */

export interface SyncStatus {
  connected: boolean;
  serverUrl: string | null;
  lastSyncAt: string | null;
  pendingChanges: number;
  openConflicts: number;
}

export interface SyncOutcome {
  pushed: number;
  pulled: number;
  applied: number;
  conflicts: number;
  deferred: number;
  cursor: number;
}

const DISCONNECTED: SyncStatus = {
  connected: false,
  serverUrl: null,
  lastSyncAt: null,
  pendingChanges: 0,
  openConflicts: 0,
};

export async function syncStatus(): Promise<SyncStatus> {
  if (!isTauri()) return DISCONNECTED;
  return (await invoke('sync_status')) as SyncStatus;
}

export async function syncConnect(code: string, deviceName: string): Promise<SyncOutcome> {
  return (await invoke('sync_connect', { code, deviceName })) as SyncOutcome;
}

export async function syncNow(): Promise<SyncOutcome> {
  return (await invoke('sync_now')) as SyncOutcome;
}

export async function syncShareCode(enrolmentSecret: string): Promise<string> {
  return (await invoke('sync_share_code', { enrolmentSecret })) as string;
}

export async function syncDisconnect(): Promise<void> {
  await invoke('sync_disconnect');
}

// ---------------------------------------------------------------------------
// Conflicts
// ---------------------------------------------------------------------------

/** One field two devices answered differently. Both answers are kept. */
export interface FieldConflict {
  field: string;
  localValue: unknown;
  remoteValue: unknown;
  localUpdatedAt: string;
  remoteUpdatedAt: string;
  localDeviceId: string | null;
  remoteDeviceId: string | null;
}

export interface OpenConflict {
  id: string;
  entityType: string;
  entityId: string;
  displayName: string | null;
  detectedAt: string;
  fields: FieldConflict[];
}

export type ConflictSide = 'local' | 'remote';

export interface FieldChoice {
  field: string;
  side: ConflictSide;
}

export async function openConflicts(): Promise<OpenConflict[]> {
  // The browser build has no sync and therefore nothing to decide. An empty
  // list rather than a thrown error, so the screen renders its empty state.
  if (!isTauri()) return [];
  return (await invoke('conflicts_open')) as OpenConflict[];
}

export async function resolveConflict(
  conflictId: string,
  choices: FieldChoice[],
): Promise<void> {
  await invoke('conflicts_resolve', { conflictId, choices });
}

/**
 * Run `onChanged` whenever the background loop actually brought something in.
 *
 * Only on real news: the loop stays quiet on a round that moved nothing, so
 * this does not turn into every screen refetching on a timer forever. Returns
 * an unsubscribe function, and a no-op one in the browser build where there is
 * no loop to listen to.
 */
export async function onSyncChanged(
  onChanged: (outcome: SyncOutcome) => void,
): Promise<() => void> {
  if (!isTauri()) return () => {};
  const { listen } = await import('@tauri-apps/api/event');
  return listen<SyncOutcome>('sync:changed', (event) => onChanged(event.payload));
}
