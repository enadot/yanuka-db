import { ROLE_PERMISSIONS, type Permission, type Principal, type User } from '@yanuka/types';

/**
 * Resolve a user's effective permissions.
 *
 * Denials are applied last and unconditionally. A permission explicitly taken
 * away from a user must not be restorable by their role or by an extra grant —
 * the safe direction for a revocation is always "off".
 */
export function resolvePermissions(user: Pick<User, 'role' | 'extraPermissions' | 'deniedPermissions'>): Permission[] {
  const effective = new Set<Permission>(ROLE_PERMISSIONS[user.role]);
  for (const permission of user.extraPermissions) effective.add(permission);
  for (const permission of user.deniedPermissions) effective.delete(permission);
  return [...effective];
}

export function toPrincipal(user: User): Principal {
  return {
    userId: user.id,
    displayName: user.displayName,
    role: user.role,
    permissions: resolvePermissions(user),
  };
}

/** Whether a principal holds a permission. The only authorization check. */
export function can(principal: Principal | null, permission: Permission): boolean {
  if (!principal) return false;
  return principal.permissions.includes(permission);
}

/** Whether any of the permissions is held. */
export function canAny(principal: Principal | null, permissions: Permission[]): boolean {
  return permissions.some((permission) => can(principal, permission));
}

/**
 * Redact fields the principal may not see.
 *
 * Applied at the boundary where data leaves the repository, not in the view, so
 * that a screen which forgets to check cannot leak a phone number.
 */
export function redactForPrincipal<
  T extends { phones?: unknown[]; contactNotes?: Array<{ isSensitive: boolean }> },
>(contact: T, principal: Principal | null): T {
  const result = { ...contact };

  if (!can(principal, 'contacts:view_phones')) {
    result.phones = [];
  }
  if (!can(principal, 'contacts:view_sensitive') && Array.isArray(result.contactNotes)) {
    result.contactNotes = result.contactNotes.filter((note) => !note.isSensitive);
  }

  return result;
}
