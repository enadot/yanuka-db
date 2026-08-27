import type { z } from 'zod';
import type {
  AuditLogEntry,
  Category,
  ContactSummary,
  ContactWithRelations,
  DeletedContactSummary,
  Note,
  Organization,
  Relationship,
  SearchResponse,
  SearchSuggestion,
  SyncState,
  Tag,
  Ulid,
} from '@yanuka/types';
import type {
  CategoryInputSchema,
  ContactInputSchema,
  ListContactsQuerySchema,
  NoteInputSchema,
  OrganizationInputSchema,
  QuickAddContactSchema,
  RelationshipInputSchema,
  SearchQuerySchema,
  TagInputSchema,
} from '@yanuka/validation';

export type ContactInput = z.infer<typeof ContactInputSchema>;
export type QuickAddInput = z.infer<typeof QuickAddContactSchema>;
export type SearchInput = z.infer<typeof SearchQuerySchema>;
export type ListContactsInput = z.infer<typeof ListContactsQuerySchema>;
export type TagInput = z.infer<typeof TagInputSchema>;
export type CategoryInput = z.infer<typeof CategoryInputSchema>;
export type OrganizationInput = z.infer<typeof OrganizationInputSchema>;
export type RelationshipInput = z.infer<typeof RelationshipInputSchema>;
export type NoteInput = z.infer<typeof NoteInputSchema>;

export interface Page<T> {
  items: T[];
  /** Opaque keyset cursor; null when there is no further page. */
  nextCursor: string | null;
  total: number;
}

/** Counts shown in the offline / sync indicator. */
export interface DatabaseStats {
  contacts: number;
  organizations: number;
  tags: number;
  relationships: number;
  notes: number;
  sync: SyncState;
}

/** Two existing contacts that are likely the same person. */
export interface DuplicatePair {
  first: ContactSummary;
  second: ContactSummary;
  /** 0-1. The strongest single signal, not a sum. */
  confidence: number;
  /** Why the pair was flagged, e.g. `אותו מספר טלפון`. */
  reasons: string[];
}

/** A contact that may already exist, surfaced before a duplicate is created. */
export interface DuplicateCandidate {
  contact: ContactSummary;
  /** 0-1. Higher means more likely the same person. */
  confidence: number;
  /** Why it was flagged, e.g. `אותו מספר טלפון`. */
  reasons: string[];
}

/**
 * The single boundary between the application and its data.
 *
 * Everything above this interface — screens, hooks, domain logic — is platform
 * independent, which is what allows the same code to run against SQLite on the
 * desktop, an in-memory store in the browser, and eventually Postgres on the
 * web. Nothing here mentions SQL, IPC or Tauri on purpose: the moment a raw
 * query string crosses this line, the web and mobile clients stop being able to
 * reuse the layer above it.
 *
 * Every implementation must pass `runRepositoryContractTests`.
 */
export interface ContactsRepository {
  // -- reads ---------------------------------------------------------------

  search(input: SearchInput): Promise<SearchResponse>;

  /** Typeahead for the Ctrl+K palette. Must stay under a keystroke's budget. */
  suggest(text: string, limit?: number): Promise<SearchSuggestion[]>;

  listContacts(input: ListContactsInput): Promise<Page<ContactSummary>>;

  getContact(id: Ulid): Promise<ContactWithRelations | null>;

  /** Records opened recently, for the home screen. */
  recentContacts(limit?: number): Promise<ContactSummary[]>;

  favoriteContacts(limit?: number): Promise<ContactSummary[]>;

  // -- contact writes ------------------------------------------------------

  /**
   * `id` is supplied by the caller so a create can be retried offline without
   * producing two records.
   */
  createContact(input: ContactInput, id?: Ulid): Promise<ContactWithRelations>;

  quickAddContact(input: QuickAddInput, id?: Ulid): Promise<ContactWithRelations>;

  /**
   * @param baseVersion version the edit was computed against; when supplied and
   * stale, the write is rejected with a `stale_version` error rather than
   * silently overwriting a concurrent change.
   */
  updateContact(
    id: Ulid,
    patch: Partial<ContactInput>,
    baseVersion?: number,
  ): Promise<ContactWithRelations>;

  /** Soft delete. The row stays so the deletion can be synced and undone. */
  deleteContact(id: Ulid): Promise<void>;

  restoreContact(id: Ulid): Promise<ContactWithRelations>;

  /** Soft-deleted contacts, newest deletion first — the trash screen. */
  listDeletedContacts(limit?: number): Promise<DeletedContactSummary[]>;

  setFavorite(id: Ulid, isFavorite: boolean): Promise<void>;

  /** Records that the contact was opened, feeding recency ranking. */
  touchContact(id: Ulid): Promise<void>;

  findDuplicates(input: Partial<ContactInput>, excludeId?: Ulid): Promise<DuplicateCandidate[]>;

  /** Scan the whole database for likely-duplicate pairs, strongest first. */
  listDuplicatePairs(limit?: number): Promise<DuplicatePair[]>;

  /**
   * Merge `mergeId` into `keepId` without losing data: children move over
   * (skipping value-duplicates), blank scalars fill from the merged contact,
   * conflicting scalars are preserved in the notes, and the merged contact is
   * soft-deleted with its full prior state in the mutation log.
   */
  mergeContacts(keepId: Ulid, mergeId: Ulid): Promise<ContactWithRelations>;

  // -- taxonomy ------------------------------------------------------------

  listTags(): Promise<Tag[]>;
  createTag(input: TagInput): Promise<Tag>;
  deleteTag(id: Ulid): Promise<void>;

  listCategories(): Promise<Category[]>;
  createCategory(input: CategoryInput): Promise<Category>;
  deleteCategory(id: Ulid): Promise<void>;

  listOrganizations(query?: string, limit?: number): Promise<Organization[]>;
  createOrganization(input: OrganizationInput): Promise<Organization>;
  deleteOrganization(id: Ulid): Promise<void>;

  // -- graph ---------------------------------------------------------------

  createRelationship(input: RelationshipInput): Promise<Relationship>;
  deleteRelationship(id: Ulid): Promise<void>;

  // -- notes ---------------------------------------------------------------

  addNote(input: NoteInput): Promise<Note>;
  updateNote(id: Ulid, body: string, isSensitive?: boolean): Promise<Note>;
  deleteNote(id: Ulid): Promise<void>;

  // -- meta ----------------------------------------------------------------

  stats(): Promise<DatabaseStats>;

  auditLog(entityId?: Ulid, limit?: number): Promise<AuditLogEntry[]>;
}
