import type { IsoDateTime, SyncableEntity, Ulid } from './primitives.js';

/** Entity kinds that participate in the mutation log and therefore in sync. */
export const SYNCABLE_ENTITY_TYPES = [
  'contact',
  'contact_phone',
  'contact_email',
  'contact_alias',
  'tag',
  'contact_tag',
  'category',
  'contact_category',
  'organization',
  'contact_organization',
  'relationship',
  'note',
] as const;
export type SyncableEntityType = (typeof SYNCABLE_ENTITY_TYPES)[number];

export const MUTATION_OPERATIONS = ['create', 'update', 'delete'] as const;
export type MutationOperation = (typeof MUTATION_OPERATIONS)[number];

export const SYNC_STATUSES = ['pending', 'syncing', 'synced', 'failed', 'conflict'] as const;
export type SyncStatus = (typeof SYNC_STATUSES)[number];

/**
 * A single local write, recorded before it is acknowledged by the server.
 *
 * `payload` holds only the fields that actually changed, which is what makes
 * field-level conflict resolution possible: two devices editing different
 * fields of the same contact merge automatically. See docs/SYNC.md.
 */
export interface Mutation {
  id: Ulid;
  entityType: SyncableEntityType;
  entityId: Ulid;
  operation: MutationOperation;
  /** Changed fields only, as a JSON object. Null for deletes. */
  payload: Record<string, unknown> | null;
  /** Values before the change, retained so a failed push can be explained or undone. */
  previous: Record<string, unknown> | null;
  /** Entity `version` this mutation was computed against. */
  baseVersion: number;
  createdAt: IsoDateTime;
  deviceId: string;
  userId: Ulid | null;
  status: SyncStatus;
  attempts: number;
  lastError: string | null;
  syncedAt: IsoDateTime | null;
}

export const DEVICE_TYPES = ['desktop', 'android', 'ios', 'web'] as const;
export type DeviceType = (typeof DEVICE_TYPES)[number];

export interface Device {
  id: string;
  name: string;
  type: DeviceType;
  platform: string | null;
  appVersion: string | null;
  lastSeenAt: IsoDateTime | null;
  lastSyncAt: IsoDateTime | null;
  revokedAt: IsoDateTime | null;
}

/** Bookmark into the server's change stream, so pulls stay incremental. */
export interface SyncCursor {
  id: string;
  entityType: SyncableEntityType | 'all';
  cursor: string | null;
  lastPulledAt: IsoDateTime | null;
  lastPushedAt: IsoDateTime | null;
}

/**
 * A field both sides changed independently. Never resolved silently — the two
 * values are kept until a human picks one. Losing data is worse than showing a
 * duplicate; see docs/SYNC.md §Conflicts.
 */
export interface FieldConflict {
  field: string;
  localValue: unknown;
  remoteValue: unknown;
  localUpdatedAt: IsoDateTime;
  remoteUpdatedAt: IsoDateTime;
  localDeviceId: string | null;
  remoteDeviceId: string | null;
}

export interface Conflict extends Pick<SyncableEntity, 'id'> {
  entityType: SyncableEntityType;
  entityId: Ulid;
  fields: FieldConflict[];
  detectedAt: IsoDateTime;
  resolvedAt: IsoDateTime | null;
  resolution: 'local' | 'remote' | 'manual' | null;
}

export const CONFLICT_RESOLUTIONS = ['local', 'remote', 'manual'] as const;
export type ConflictResolution = (typeof CONFLICT_RESOLUTIONS)[number];

/** An open conflict as the conflicts screen lists it: with the record's name. */
export interface ConflictView extends Conflict {
  /** Display name, note wording or tag name — whatever names the record. */
  entityLabel: string | null;
  /** The other device, when the server knew its name. */
  remoteDeviceName: string | null;
}

/**
 * What the settings screen and the indicator show about the pairing.
 * `configured: false` is the normal state of a machine that has never been
 * paired — nothing is wrong, there is simply no server. (`SyncStatus` above
 * is the per-mutation state; this is the whole picture.)
 */
export interface SyncOverview extends SyncState {
  configured: boolean;
  serverUrl: string | null;
  serverName: string | null;
  deviceId: string;
  deviceName: string | null;
  /** The last cycle's failure, in the user's language; null after a good one. */
  lastError: string | null;
}

/** A one-time code minted for pairing another device. */
export interface PairCode {
  code: string;
  expiresAt: IsoDateTime;
}

/** Snapshot the desktop shows in its offline indicator. */
export interface SyncState {
  online: boolean;
  lastSyncAt: IsoDateTime | null;
  pendingMutations: number;
  failedMutations: number;
  openConflicts: number;
  syncing: boolean;
}
