import { monotonicFactory, ulid as randomUlid, decodeTime } from 'ulid';
import type { Ulid } from '@yanuka/types';

/**
 * Monotonic generator: IDs minted within the same millisecond still sort in
 * creation order. That matters because the mutation log is drained in ID order.
 */
const monotonic = monotonicFactory();

/** Mint a new ULID. Safe to call offline — no coordination with a server. */
export function newId(): Ulid {
  return monotonic();
}

/** Mint a ULID without monotonic guarantees. Used only in tests and seeds. */
export function newRandomId(): Ulid {
  return randomUlid();
}

const ULID_PATTERN = /^[0-7][0-9A-HJKMNP-TV-Z]{25}$/;

export function isUlid(value: unknown): value is Ulid {
  return typeof value === 'string' && ULID_PATTERN.test(value);
}

/** Creation time embedded in a ULID's first 48 bits. */
export function ulidTimestamp(id: Ulid): Date {
  return new Date(decodeTime(id));
}
