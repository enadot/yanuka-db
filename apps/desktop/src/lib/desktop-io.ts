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

/**
 * Encryption at rest is a property of the desktop's SQLite file; the browser
 * build holds everything in memory and reports itself as such.
 */
export interface SecurityStatus {
  state: 'encrypted' | 'plaintext' | 'locked' | 'browser';
  keyPersisted: boolean;
}

export async function securityStatus(): Promise<SecurityStatus> {
  if (!isTauri()) {
    return { state: 'browser', keyPersisted: false };
  }
  return (await invoke('security_status')) as SecurityStatus;
}

/**
 * Semantic search runs on the desktop's bundled embedding model; the browser
 * build has no model and reports itself as such.
 */
export interface SemanticStatus {
  state: 'ready' | 'indexing' | 'unavailable' | 'browser';
  indexed: number;
  pending: number;
}

export async function semanticStatus(): Promise<SemanticStatus> {
  if (!isTauri()) {
    return { state: 'browser', indexed: 0, pending: 0 };
  }
  return (await invoke('semantic_status')) as SemanticStatus;
}

/** The recovery key in display form, or null when the database is not encrypted. */
export async function recoveryKey(): Promise<string | null> {
  if (!isTauri()) {
    return null;
  }
  const answer = (await invoke('recovery_key')) as { key: string | null };
  return answer.key;
}

/** Open a locked database with a typed recovery key. Throws on a wrong key. */
export async function unlockDatabase(key: string): Promise<SecurityStatus> {
  return (await invoke('unlock_database', { key })) as SecurityStatus;
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
