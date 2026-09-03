import { invoke } from '@tauri-apps/api/core';
import { toRepositoryError, type CategoryMembersOptions, type ContactsRepository, type DatabaseStats, type DuplicateCandidate, type DuplicatePair, type ListContactsInput, type Page, type SearchInput } from '@yanuka/core';
import type {
  AuditLogEntry,
  Category,
  CategoryMembersPage,
  CategoryMembershipMode,
  CategoryPreview,
  CategoryRule,
  CategorySuggestion,
  CategorySummary,
  ContactSummary,
  ContactWithRelations,
  DeletedContactSummary,
  Note,
  Organization,
  Relationship,
  SearchResponse,
  SearchSuggestion,
  Tag,
  Ulid,
} from '@yanuka/types';
import type {
  CategoryInput,
  ContactInput,
  NoteInput,
  OrganizationInput,
  QuickAddInput,
  RelationshipInput,
  TagInput,
} from '@yanuka/core';

/**
 * Detect whether the page is running inside a Tauri window.
 *
 * Tauri v2 injects `__TAURI_INTERNALS__` before any application code runs, so
 * this is reliable at module scope — unlike a `try { invoke() }` probe, which
 * would be async and would have to fail once first.
 */
export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/**
 * Repository backed by the Rust shell and the local SQLite database.
 *
 * Every method is a thin call across the IPC boundary. No SQL is constructed
 * here: the frontend names an operation and passes typed arguments, and the
 * Rust side owns the query, the transaction and the mutation-log entry. That
 * keeps the database dialect out of the UI entirely, which is what lets the
 * same screens run against the in-memory repository and, later, against
 * Postgres from the web client.
 *
 * The Rust side re-validates every argument. A webview is not a trust boundary
 * we can rely on, so client-side validation is a convenience, never a control.
 */
export class TauriRepository implements ContactsRepository {
  private async call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    try {
      return (await invoke(command, args)) as T;
    } catch (error) {
      // Errors arrive as plain serialized objects, not Error instances.
      throw toRepositoryError(error);
    }
  }

  search(input: SearchInput): Promise<SearchResponse> {
    return this.call('search_contacts', { input });
  }

  suggest(text: string, limit = 8): Promise<SearchSuggestion[]> {
    return this.call('suggest_contacts', { text, limit });
  }

  listContacts(input: ListContactsInput): Promise<Page<ContactSummary>> {
    return this.call('list_contacts', { input });
  }

  getContact(id: Ulid): Promise<ContactWithRelations | null> {
    return this.call('get_contact', { id });
  }

  recentContacts(limit = 8): Promise<ContactSummary[]> {
    return this.call('recent_contacts', { limit });
  }

  favoriteContacts(limit = 12): Promise<ContactSummary[]> {
    return this.call('favorite_contacts', { limit });
  }

  createContact(input: ContactInput, id?: Ulid): Promise<ContactWithRelations> {
    return this.call('create_contact', { input, id });
  }

  quickAddContact(input: QuickAddInput, id?: Ulid): Promise<ContactWithRelations> {
    return this.call('quick_add_contact', { input, id });
  }

  updateContact(
    id: Ulid,
    patch: Partial<ContactInput>,
    baseVersion?: number,
  ): Promise<ContactWithRelations> {
    return this.call('update_contact', { id, patch, baseVersion });
  }

  deleteContact(id: Ulid): Promise<void> {
    return this.call('delete_contact', { id });
  }

  restoreContact(id: Ulid): Promise<ContactWithRelations> {
    return this.call('restore_contact', { id });
  }

  listDeletedContacts(limit = 50): Promise<DeletedContactSummary[]> {
    return this.call('list_deleted_contacts', { limit });
  }

  setFavorite(id: Ulid, isFavorite: boolean): Promise<void> {
    return this.call('set_favorite', { id, isFavorite });
  }

  touchContact(id: Ulid): Promise<void> {
    return this.call('touch_contact', { id });
  }

  findDuplicates(input: Partial<ContactInput>, excludeId?: Ulid): Promise<DuplicateCandidate[]> {
    return this.call('find_duplicates', { input, excludeId });
  }

  listDuplicatePairs(limit = 100): Promise<DuplicatePair[]> {
    return this.call('list_duplicate_pairs', { limit });
  }

  mergeContacts(keepId: Ulid, mergeId: Ulid): Promise<ContactWithRelations> {
    return this.call('merge_contacts', { keepId, mergeId });
  }

  listTags(): Promise<Tag[]> {
    return this.call('list_tags');
  }

  createTag(input: TagInput): Promise<Tag> {
    return this.call('create_tag', { input });
  }

  deleteTag(id: Ulid): Promise<void> {
    return this.call('delete_tag', { id });
  }

  listCategories(): Promise<CategorySummary[]> {
    return this.call('list_categories');
  }

  getCategory(id: Ulid): Promise<CategorySummary | null> {
    return this.call('get_category', { id });
  }

  createCategory(input: CategoryInput): Promise<Category> {
    return this.call('create_category', { input });
  }

  updateCategory(id: Ulid, input: CategoryInput): Promise<Category> {
    return this.call('update_category', { id, input });
  }

  deleteCategory(id: Ulid): Promise<void> {
    return this.call('delete_category', { id });
  }

  reorderCategories(ids: Ulid[]): Promise<void> {
    return this.call('reorder_categories', { ids });
  }

  categoryMembers(id: Ulid, options: CategoryMembersOptions = {}): Promise<CategoryMembersPage> {
    return this.call('category_members', {
      id,
      query: options.query ?? null,
      limit: options.limit ?? 100,
      offset: options.offset ?? 0,
    });
  }

  previewCategoryRule(rule: CategoryRule): Promise<CategoryPreview> {
    return this.call('preview_category_rule', { rule });
  }

  setCategoryMembership(
    categoryId: Ulid,
    contactId: Ulid,
    mode: CategoryMembershipMode,
  ): Promise<void> {
    return this.call('set_category_membership', { categoryId, contactId, mode });
  }

  suggestCategories(): Promise<CategorySuggestion[]> {
    return this.call('suggest_categories');
  }

  listOrganizations(query?: string, limit = 50): Promise<Organization[]> {
    return this.call('list_organizations', { query, limit });
  }

  createOrganization(input: OrganizationInput): Promise<Organization> {
    return this.call('create_organization', { input });
  }

  deleteOrganization(id: Ulid): Promise<void> {
    return this.call('delete_organization', { id });
  }

  createRelationship(input: RelationshipInput): Promise<Relationship> {
    return this.call('create_relationship', { input });
  }

  deleteRelationship(id: Ulid): Promise<void> {
    return this.call('delete_relationship', { id });
  }

  addNote(input: NoteInput): Promise<Note> {
    return this.call('add_note', { input });
  }

  updateNote(id: Ulid, body: string, isSensitive?: boolean): Promise<Note> {
    return this.call('update_note', { id, body, isSensitive });
  }

  deleteNote(id: Ulid): Promise<void> {
    return this.call('delete_note', { id });
  }

  stats(): Promise<DatabaseStats> {
    return this.call('database_stats');
  }

  auditLog(entityId?: Ulid, limit = 50): Promise<AuditLogEntry[]> {
    return this.call('audit_log', { entityId, limit });
  }
}

/**
 * Every IPC command this client can issue.
 *
 * Exported so a test can assert that the set matches the `#[tauri::command]`
 * functions registered in the Rust shell. A rename on one side without the
 * other is otherwise only discovered at runtime, in a build that cannot be run
 * on a Linux CI machine.
 */
export const IPC_COMMANDS = [
  'search_contacts',
  'suggest_contacts',
  'list_contacts',
  'get_contact',
  'recent_contacts',
  'favorite_contacts',
  'create_contact',
  'quick_add_contact',
  'update_contact',
  'delete_contact',
  'restore_contact',
  'list_deleted_contacts',
  'set_favorite',
  'touch_contact',
  'find_duplicates',
  'list_duplicate_pairs',
  'merge_contacts',
  'list_tags',
  'create_tag',
  'delete_tag',
  'list_categories',
  'get_category',
  'create_category',
  'update_category',
  'delete_category',
  'reorder_categories',
  'category_members',
  'preview_category_rule',
  'set_category_membership',
  'suggest_categories',
  'list_organizations',
  'create_organization',
  'delete_organization',
  'create_relationship',
  'delete_relationship',
  'add_note',
  'update_note',
  'delete_note',
  'database_stats',
  'backup_database',
  'backup_status',
  'security_status',
  'semantic_status',
  'recovery_key',
  'unlock_database',
  'ocr_import_page',
  'ocr_list_pages',
  'ocr_get_page',
  'ocr_set_token_text',
  'ocr_lexicon',
  'ocr_save_note',
  'ocr_delete_page',
  'save_exported_csv',
  'audit_log',
] as const;
