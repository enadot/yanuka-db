import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import type {
  ContactInput,
  ListContactsInput,
  NoteInput,
  OrganizationInput,
  RelationshipInput,
  SearchInput,
} from '@yanuka/core';
import type { Ulid } from '@yanuka/types';
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
  organizations: (query?: string) => ['organizations', query ?? ''] as const,
  stats: () => ['stats'] as const,
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

export function useOrganizations(query?: string) {
  const repository = useRepository();
  return useQuery({
    queryKey: queryKeys.organizations(query),
    queryFn: () => repository.listOrganizations(query),
  });
}

export function useDeletedContacts() {
  const repository = useRepository();
  return useQuery({
    queryKey: ['contacts', 'deleted'],
    queryFn: () => repository.deletedContacts(),
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

export function useDeleteContact() {
  const repository = useRepository();
  const invalidate = useInvalidateContacts();
  return useMutation({
    mutationFn: (id: Ulid) => repository.deleteContact(id),
    onSuccess: invalidate,
  });
}

export function useCreateOrganization() {
  const repository = useRepository();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (input: OrganizationInput) => repository.createOrganization(input),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['organizations'] });
      void queryClient.invalidateQueries({ queryKey: ['stats'] });
    },
  });
}

/**
 * Relationship and note writes.
 *
 * They invalidate the same broad set as a contact edit because an edge belongs
 * to two contacts and a note is indexed for search — narrowing this would show
 * the far side of a new relationship a stale card.
 */
export function useCreateRelationship() {
  const repository = useRepository();
  const invalidate = useInvalidateContacts();
  return useMutation({
    mutationFn: (input: RelationshipInput) => repository.createRelationship(input),
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

export function useAddNote() {
  const repository = useRepository();
  const invalidate = useInvalidateContacts();
  return useMutation({
    mutationFn: (input: NoteInput) => repository.addNote(input),
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

export function useSetFavorite() {
  const repository = useRepository();
  const invalidate = useInvalidateContacts();
  return useMutation({
    mutationFn: ({ id, isFavorite }: { id: Ulid; isFavorite: boolean }) =>
      repository.setFavorite(id, isFavorite),
    onSuccess: invalidate,
  });
}
