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

// ---------------------------------------------------------------------------
// Notebook import (ADR-037).
//
// Desktop-only capability, like backups and encryption: the segmentation and
// the writer memory live in Rust. The browser build serves one demo page with
// simulated learning, so the workbench is fully exercisable in e2e.
// ---------------------------------------------------------------------------

export interface OcrToken {
  id: string;
  lineIndex: number;
  tokenIndex: number;
  x: number;
  y: number;
  w: number;
  h: number;
  text: string | null;
  source: 'none' | 'learned' | 'manual';
  confidence: number | null;
}

export interface OcrPageSummary {
  id: string;
  fileName: string;
  status: string;
  contactId: string | null;
  width: number;
  height: number;
  tokens: number;
  filled: number;
  importedAt: string;
}

export interface OcrPageDetail {
  id: string;
  fileName: string;
  status: string;
  contactId: string | null;
  width: number;
  height: number;
  imageDataUrl: string;
  tokens: OcrToken[];
}

export function ocrAvailable(): boolean {
  return isTauri();
}

/** Demo state for the browser build: one page, two lines, one repeated shape. */
const demoTokens: OcrToken[] = [
  { id: 'demo-1', lineIndex: 0, tokenIndex: 0, x: 480, y: 30, w: 120, h: 40, text: null, source: 'none', confidence: null },
  { id: 'demo-2', lineIndex: 0, tokenIndex: 1, x: 320, y: 30, w: 120, h: 40, text: null, source: 'none', confidence: null },
  { id: 'demo-3', lineIndex: 1, tokenIndex: 0, x: 480, y: 110, w: 120, h: 40, text: null, source: 'none', confidence: null },
];
// demo-1 and demo-3 are "the same shape" — correcting one teaches the other.
const demoTwins: Record<string, string> = { 'demo-1': 'demo-3', 'demo-3': 'demo-1' };

function demoImage(): string {
  const boxes = demoTokens
    .map((t) => `<rect x="${t.x}" y="${t.y}" width="${t.w}" height="${t.h}" fill="#cbd5e1"/>`)
    .join('');
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="640" height="200"><rect width="640" height="200" fill="#f8fafc"/>${boxes}</svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

export async function ocrImportPage(fileName: string, dataBase64: string): Promise<string> {
  if (!isTauri()) {
    throw new Error('ייבוא מחברות זמין באפליקציית המחשב');
  }
  const result = (await invoke('ocr_import_page', { fileName, dataBase64 })) as { id: string };
  return result.id;
}

export async function ocrListPages(): Promise<OcrPageSummary[]> {
  if (!isTauri()) {
    return [
      {
        id: 'demo-page',
        fileName: 'מחברת-הדגמה.png',
        status: 'new',
        contactId: null,
        width: 640,
        height: 200,
        tokens: demoTokens.length,
        filled: demoTokens.filter((t) => t.text).length,
        importedAt: new Date().toISOString(),
      },
    ];
  }
  return (await invoke('ocr_list_pages')) as OcrPageSummary[];
}

export async function ocrGetPage(id: string): Promise<OcrPageDetail> {
  if (!isTauri()) {
    return {
      id: 'demo-page',
      fileName: 'מחברת-הדגמה.png',
      status: 'new',
      contactId: null,
      width: 640,
      height: 200,
      imageDataUrl: demoImage(),
      tokens: demoTokens.map((t) => ({ ...t })),
    };
  }
  return (await invoke('ocr_get_page', { id })) as OcrPageDetail;
}

export async function ocrSetTokenText(tokenId: string, text: string): Promise<OcrToken[]> {
  if (!isTauri()) {
    const token = demoTokens.find((t) => t.id === tokenId);
    if (token) {
      token.text = text.trim() || null;
      token.source = token.text ? 'manual' : 'none';
      const twin = demoTokens.find((t) => t.id === demoTwins[tokenId]);
      if (token.text && twin && twin.source !== 'manual') {
        twin.text = token.text;
        twin.source = 'learned';
        twin.confidence = 0.97;
      }
    }
    return demoTokens.map((t) => ({ ...t }));
  }
  return (await invoke('ocr_set_token_text', { tokenId, text })) as OcrToken[];
}

export async function ocrLexicon(prefix: string): Promise<string[]> {
  if (!isTauri()) {
    return prefix ? ['אברהם כהן', 'ירושלים'].filter((t) => t.startsWith(prefix)) : [];
  }
  return (await invoke('ocr_lexicon', { prefix })) as string[];
}

export async function ocrSaveNote(pageId: string, contactId: string): Promise<string> {
  if (!isTauri()) {
    throw new Error('שמירת הערה מדף זמינה באפליקציית המחשב');
  }
  const result = (await invoke('ocr_save_note', { pageId, contactId })) as { noteId: string };
  return result.noteId;
}

export async function ocrDeletePage(id: string): Promise<void> {
  if (!isTauri()) {
    return;
  }
  await invoke('ocr_delete_page', { id });
}
