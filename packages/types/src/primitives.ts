/**
 * Primitive aliases shared by every entity in the system.
 *
 * IDs are ULIDs (26-char Crockford base32, lexicographically sortable by
 * creation time). Auto-increment integers are forbidden: every device must be
 * able to mint IDs while offline without coordinating with a server.
 * See docs/DECISIONS.md — ADR-004.
 */
export type Ulid = string;

/** ISO-8601 timestamp, always stored in UTC (e.g. `2026-08-17T11:30:00.000Z`). */
export type IsoDateTime = string;

/** ISO 3166-1 alpha-2 country code, uppercase (e.g. `IL`, `GB`, `US`). */
export type CountryCode = string;

/** ISO 639-1 language code, lowercase (e.g. `he`, `en`, `fr`, `ru`, `es`). */
export type LanguageCode = string;

/**
 * Fields carried by every syncable entity.
 *
 * `version` is a per-record counter bumped on each local write; it is what the
 * sync engine compares to detect concurrent edits. `deviceId` records which
 * installation produced the current revision. `deletedAt` implements soft
 * delete so that deletions can propagate — see docs/SYNC.md.
 */
export interface SyncableEntity {
  id: Ulid;
  createdAt: IsoDateTime;
  updatedAt: IsoDateTime;
  createdBy: Ulid | null;
  updatedBy: Ulid | null;
  version: number;
  deviceId: string | null;
  deletedAt: IsoDateTime | null;
}
