import type { IsoDateTime, SyncableEntity, Ulid } from './primitives.js';

/**
 * Permissions are the unit of authorization; roles are just named bundles of
 * them. Checks in application code always test a permission, never a role, so
 * that the role set can change without touching call sites.
 */
export const PERMISSIONS = [
  'contacts:view',
  'contacts:view_phones',
  'contacts:view_sensitive',
  'contacts:create',
  'contacts:edit',
  'contacts:delete',
  'contacts:merge',
  'contacts:export',
  'contacts:import',
  'tags:manage',
  'categories:manage',
  'organizations:manage',
  'relationships:manage',
  'users:manage',
  'devices:manage',
  'audit:view',
  'settings:manage',
] as const;
export type Permission = (typeof PERMISSIONS)[number];

export const ROLES = [
  'super_admin',
  'admin',
  'editor',
  'viewer',
  'restricted_viewer',
] as const;
export type RoleName = (typeof ROLES)[number];

/**
 * Default permission grants per role.
 *
 * `restricted_viewer` deliberately lacks `contacts:view_phones` and
 * `contacts:view_sensitive`: it can find a person and see the context, but not
 * dial them or read private remarks.
 */
export const ROLE_PERMISSIONS: Record<RoleName, readonly Permission[]> = {
  super_admin: PERMISSIONS,
  admin: [
    'contacts:view',
    'contacts:view_phones',
    'contacts:view_sensitive',
    'contacts:create',
    'contacts:edit',
    'contacts:delete',
    'contacts:merge',
    'contacts:export',
    'contacts:import',
    'tags:manage',
    'categories:manage',
    'organizations:manage',
    'relationships:manage',
    'devices:manage',
    'audit:view',
    'settings:manage',
  ],
  editor: [
    'contacts:view',
    'contacts:view_phones',
    'contacts:create',
    'contacts:edit',
    'contacts:import',
    'tags:manage',
    'categories:manage',
    'organizations:manage',
    'relationships:manage',
  ],
  viewer: ['contacts:view', 'contacts:view_phones'],
  restricted_viewer: ['contacts:view'],
};

export interface User extends SyncableEntity {
  email: string;
  displayName: string;
  role: RoleName;
  /** Grants beyond the role's defaults. */
  extraPermissions: Permission[];
  /** Grants removed from the role's defaults. Revocation wins over grant. */
  deniedPermissions: Permission[];
  isActive: boolean;
  lastLoginAt: IsoDateTime | null;
}

/** The subject of an authorization check, as held by the client at runtime. */
export interface Principal {
  userId: Ulid;
  displayName: string;
  role: RoleName;
  permissions: Permission[];
}

export const AUDIT_ACTIONS = [
  'create',
  'update',
  'delete',
  'restore',
  'merge',
  'view_sensitive',
  'export',
  'import',
  'login',
  'sync',
] as const;
export type AuditAction = (typeof AUDIT_ACTIONS)[number];

/** Who did what, when, from which device, to which record. Append-only. */
export interface AuditLogEntry {
  id: Ulid;
  userId: Ulid | null;
  userDisplayName: string | null;
  action: AuditAction;
  entityType: string;
  entityId: Ulid | null;
  entityLabel: string | null;
  /** Field-level before/after, omitted for read actions. */
  changes: Record<string, { from: unknown; to: unknown }> | null;
  deviceId: string | null;
  deviceName: string | null;
  createdAt: IsoDateTime;
}
