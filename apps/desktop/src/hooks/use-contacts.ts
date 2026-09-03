import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import type { CategoryInput, ContactInput, ListContactsInput, SearchInput } from '@yanuka/core';
import type { CategoryMembershipMode, CategoryRule, RelationshipType, Ulid } from '@yanuka/types';
import { NoteInputSchema, RelationshipInputSchema } from '@yanuka/validation';
import { useRepository } from '../lib/repository';

/** Query key namespaces, so a write can invalidate exactly what it affected. */
export const queryKeys = {
  search: (input: SearchInput) => ['search', input] as const,
  suggest: (text: string) => ['suggest', text] as const,
  list: (input: ListContactsInput) => ['contacts', 'list', input] as const,
  contact: (id: Ulid) => ['contacts', 'detail', id] as const,
  recent: () => ['contacts', 'recent'] as const,
  favorites: () => ['contacts', 'favorites'] as const,
  tags: () => ['tags'] as const,
  categories: () => ['categories'] as const,
  category: (id: Ulid) => ['categories', 'detail', id] as const,
  categoryMembers: (id: Ulid, query: string) => ['categories', 'members', id, query] as const,
  categorySuggestions: () => ['categories', 'suggestions'] as const,
  categoryPreview: (rule: CategoryRule | null) =>
    ['categories', 'preview', JSON.stringify(rule)] as const,
  organizations: (query?: string) => ['organizations', query ?? ''] as const,
  stats: () => ['stats'] as const,
  trash: () => ['contacts', 'trash'] as const,
  history: (id: Ulid) => ['contacts', 'history', id] as const,
};

export function useSearch(input: SearchInput, enabled = true) {
  const repository = useRepository();
  return useQuery({
    queryKey: queryKeys.search(input),
    queryFn: () => repository.search(input),
    enabled,
    // Keeping the previous page visible while the next one loads avoids the
    // list collapsing to empty on every keystroke.
    placeholderData: (previous) => previous,
  });
}

export function useSuggestions(text: string) {
  const repository = useRepository();
  return useQuery({
    queryKey: queryKeys.suggest(text),
    queryFn: () => repository.suggest(text, 8),
    enabled: text.trim().length > 0,
    placeholderData: (previous) => previous,
  });
}

export function useContactList(input: ListContactsInput) {
  const repository = useRepository();
  return useQuery({
    queryKey: queryKeys.list(input),
    queryFn: () => repository.listContacts(input),
    placeholderData: (previous) => previous,
  });
}

export function useContact(id: Ulid | undefined) {
  const repository = useRepository();
  return useQuery({
    queryKey: queryKeys.contact(id ?? ''),
    queryFn: () => repository.getContact(id!),
    enabled: Boolean(id),
  });
}

export function useRecentContacts() {
  const repository = useRepository();
  return useQuery({ queryKey: queryKeys.recent(), queryFn: () => repository.recentContacts(6) });
}

export function useFavoriteContacts() {
  const repository = useRepository();
  return useQuery({
    queryKey: queryKeys.favorites(),
    queryFn: () => repository.favoriteContacts(8),
  });
}

export function useTags() {
  const repository = useRepository();
  return useQuery({ queryKey: queryKeys.tags(), queryFn: () => repository.listTags() });
}

export function useCategories() {
  const repository = useRepository();
  return useQuery({ queryKey: queryKeys.categories(), queryFn: () => repository.listCategories() });
}

// -- smart categories (ADR-038) ---------------------------------------------

export function useCategory(id: Ulid | undefined) {
  const repository = useRepository();
  return useQuery({
    queryKey: queryKeys.category(id ?? ''),
    queryFn: () => repository.getCategory(id!),
    enabled: Boolean(id),
  });
}

export function useCategoryMembers(id: Ulid | undefined, query = '') {
  const repository = useRepository();
  return useQuery({
    queryKey: queryKeys.categoryMembers(id ?? '', query),
    queryFn: () => repository.categoryMembers(id!, { query: query || undefined, limit: 200 }),
    enabled: Boolean(id),
    placeholderData: (previous) => previous,
  });
}

export function useCategorySuggestions() {
  const repository = useRepository();
  return useQuery({
    queryKey: queryKeys.categorySuggestions(),
    queryFn: () => repository.suggestCategories(),
  });
}

/** Live "who would this select" while a rule is being edited. */
export function useCategoryPreview(rule: CategoryRule | null) {
  const repository = useRepository();
  return useQuery({
    queryKey: queryKeys.categoryPreview(rule),
    queryFn: () => repository.previewCategoryRule(rule!),
    enabled: rule != null && rule.conditions.length > 0,
    placeholderData: (previous) => previous,
  });
}

/**
 * A category write changes tiles, counts, contact cards and search facets at
 * once; invalidating both namespaces is cheaper than being wrong about one.
 */
function useInvalidateCategories() {
  const queryClient = useQueryClient();
  return () => {
    void queryClient.invalidateQueries({ queryKey: ['categories'] });
    void queryClient.invalidateQueries({ queryKey: ['contacts'] });
    void queryClient.invalidateQueries({ queryKey: ['search'] });
  };
}

export function useCreateCategory() {
  const repository = useRepository();
  const invalidate = useInvalidateCategories();
  return useMutation({
    mutationFn: (input: CategoryInput) => repository.createCategory(input),
    onSuccess: invalidate,
  });
}

export function useUpdateCategory() {
  const repository = useRepository();
  const invalidate = useInvalidateCategories();
  return useMutation({
    mutationFn: ({ id, input }: { id: Ulid; input: CategoryInput }) =>
      repository.updateCategory(id, input),
    onSuccess: invalidate,
  });
}

export function useDeleteCategory() {
  const repository = useRepository();
  const invalidate = useInvalidateCategories();
  return useMutation({
    mutationFn: (id: Ulid) => repository.deleteCategory(id),
    onSuccess: invalidate,
  });
}

export function useReorderCategories() {
  const repository = useRepository();
  const invalidate = useInvalidateCategories();
  return useMutation({
    mutationFn: (ids: Ulid[]) => repository.reorderCategories(ids),
    onSuccess: invalidate,
  });
}

export function useSetCategoryMembership() {
  const repository = useRepository();
  const invalidate = useInvalidateCategories();
  return useMutation({
    mutationFn: ({
      categoryId,
      contactId,
      mode,
    }: {
      categoryId: Ulid;
      contactId: Ulid;
      mode: CategoryMembershipMode;
    }) => repository.setCategoryMembership(categoryId, contactId, mode),
    onSuccess: invalidate,
  });
}

export function useOrganizations(query?: string) {
  const repository = useRepository();
  return useQuery({
    queryKey: queryKeys.organizations(query),
    queryFn: () => repository.listOrganizations(query),
  });
}

export function useDatabaseStats() {
  const repository = useRepository();
  return useQuery({ queryKey: queryKeys.stats(), queryFn: () => repository.stats() });
}

/**
 * Invalidate everything a contact write can affect.
 *
 * Deliberately broad: a single edit changes list ordering, search ranking,
 * facet counts and the stats footer. Being precise here would trade a
 * negligible saving against the risk of showing stale data, and stale contact
 * data is the one thing this application must not do.
 */
function useInvalidateContacts() {
  const queryClient = useQueryClient();
  return () => {
    void queryClient.invalidateQueries({ queryKey: ['contacts'] });
    void queryClient.invalidateQueries({ queryKey: ['search'] });
    void queryClient.invalidateQueries({ queryKey: ['suggest'] });
    void queryClient.invalidateQueries({ queryKey: ['stats'] });
  };
}

/**
 * Bulk-import surface: create without per-row invalidation.
 *
 * `useCreateContact` invalidates four query namespaces on every success, which
 * is right for a form and wrong for a three-hundred-row import. The import
 * screen creates rows through `create` and calls `invalidate` once at the end.
 */
export function useImportContacts() {
  const repository = useRepository();
  const invalidate = useInvalidateContacts();
  return {
    create: (input: ContactInput) => repository.createContact(input),
    invalidate,
  };
}

export function useDuplicatePairs() {
  const repository = useRepository();
  return useQuery({
    queryKey: ['contacts', 'duplicate-pairs'],
    queryFn: () => repository.listDuplicatePairs(),
  });
}

export function useMergeContacts() {
  const repository = useRepository();
  const invalidate = useInvalidateContacts();
  return useMutation({
    mutationFn: ({ keepId, mergeId }: { keepId: Ulid; mergeId: Ulid }) =>
      repository.mergeContacts(keepId, mergeId),
    onSuccess: invalidate,
  });
}

export function useCreateContact() {
  const repository = useRepository();
  const invalidate = useInvalidateContacts();
  return useMutation({
    mutationFn: (input: ContactInput) => repository.createContact(input),
    onSuccess: invalidate,
  });
}

export function useUpdateContact() {
  const repository = useRepository();
  const invalidate = useInvalidateContacts();
  return useMutation({
    mutationFn: ({
      id,
      patch,
      baseVersion,
    }: {
      id: Ulid;
      patch: Partial<ContactInput>;
      baseVersion?: number;
    }) => repository.updateContact(id, patch, baseVersion),
    onSuccess: invalidate,
  });
}

export function useDeletedContacts() {
  const repository = useRepository();
  return useQuery({
    queryKey: queryKeys.trash(),
    queryFn: () => repository.listDeletedContacts(100),
  });
}

export function useRestoreContact() {
  const repository = useRepository();
  const invalidate = useInvalidateContacts();
  return useMutation({
    mutationFn: (id: Ulid) => repository.restoreContact(id),
    onSuccess: invalidate,
  });
}

/**
 * The contact's history, straight from the mutation journal: every entry
 * carries the fields that changed and the values they replaced.
 */
export function useContactHistory(id: Ulid | undefined) {
  const repository = useRepository();
  return useQuery({
    queryKey: queryKeys.history(id ?? ''),
    queryFn: () => repository.auditLog(id!, 50),
    enabled: Boolean(id),
  });
}

export function useDeleteContact() {
  const repository = useRepository();
  const invalidate = useInvalidateContacts();
  return useMutation({
    mutationFn: (id: Ulid) => repository.deleteContact(id),
    onSuccess: invalidate,
  });
}

/**
 * Notes and relationships, written from the contact card.
 *
 * Inputs are parsed here, at the UI boundary, so both repositories receive the
 * same validated shape — the in-memory mock in the browser and the SQLite
 * layer through IPC — and a bad input fails with the schema's Hebrew message
 * instead of reaching either backend.
 */
export function useAddNote() {
  const repository = useRepository();
  const invalidate = useInvalidateContacts();
  return useMutation({
    mutationFn: (input: { contactId: Ulid; body: string }) =>
      repository.addNote(NoteInputSchema.parse(input)),
    onSuccess: invalidate,
  });
}

export function useUpdateNote() {
  const repository = useRepository();
  const invalidate = useInvalidateContacts();
  return useMutation({
    mutationFn: ({ id, body }: { id: Ulid; body: string }) => repository.updateNote(id, body),
    onSuccess: invalidate,
  });
}

export function useDeleteNote() {
  const repository = useRepository();
  const invalidate = useInvalidateContacts();
  return useMutation({
    mutationFn: (id: Ulid) => repository.deleteNote(id),
    onSuccess: invalidate,
  });
}

export function useCreateRelationship() {
  const repository = useRepository();
  const invalidate = useInvalidateContacts();
  return useMutation({
    mutationFn: (input: {
      fromContactId: Ulid;
      toContactId: Ulid;
      type: RelationshipType;
      notes?: string;
    }) => repository.createRelationship(RelationshipInputSchema.parse(input)),
    onSuccess: invalidate,
  });
}

export function useDeleteRelationship() {
  const repository = useRepository();
  const invalidate = useInvalidateContacts();
  return useMutation({
    mutationFn: (id: Ulid) => repository.deleteRelationship(id),
    onSuccess: invalidate,
  });
}

export function useSetFavorite() {
  const repository = useRepository();
  const invalidate = useInvalidateContacts();
  return useMutation({
    mutationFn: ({ id, isFavorite }: { id: Ulid; isFavorite: boolean }) =>
      repository.setFavorite(id, isFavorite),
    onSuccess: invalidate,
  });
}
