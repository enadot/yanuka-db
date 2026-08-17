import type { IsoDateTime } from '@yanuka/types';

/**
 * Current time as an ISO-8601 UTC string.
 *
 * Every timestamp written to the database goes through here. Storing UTC and
 * formatting to local time only at render time is what keeps `updated_at`
 * comparisons meaningful when devices sit in different time zones.
 */
export function nowIso(): IsoDateTime {
  return new Date().toISOString();
}

export function toIso(date: Date): IsoDateTime {
  return date.toISOString();
}

export function parseIso(value: IsoDateTime): Date {
  return new Date(value);
}

/** `true` when `a` is strictly later than `b`. Null sorts earliest. */
export function isAfter(a: IsoDateTime | null, b: IsoDateTime | null): boolean {
  if (a == null) return false;
  if (b == null) return true;
  return a > b;
}

const HE_DATE = new Intl.DateTimeFormat('he-IL', {
  day: '2-digit',
  month: '2-digit',
  year: 'numeric',
});

const HE_DATE_TIME = new Intl.DateTimeFormat('he-IL', {
  day: '2-digit',
  month: '2-digit',
  year: 'numeric',
  hour: '2-digit',
  minute: '2-digit',
});

/** `17/08/2026` in the user's local zone. */
export function formatDate(value: IsoDateTime | null): string {
  if (!value) return '—';
  return HE_DATE.format(new Date(value));
}

/** `17/08/2026, 14:30` in the user's local zone. */
export function formatDateTime(value: IsoDateTime | null): string {
  if (!value) return '—';
  return HE_DATE_TIME.format(new Date(value));
}

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/**
 * Short Hebrew relative time, used by the sync indicator ("לפני 5 דקות").
 * Falls back to an absolute date beyond a week, where "לפני 23 ימים" stops
 * being easier to read than the date itself.
 */
export function formatRelative(value: IsoDateTime | null, now: Date = new Date()): string {
  if (!value) return 'מעולם לא';
  const delta = now.getTime() - new Date(value).getTime();
  if (delta < MINUTE) return 'הרגע';
  if (delta < HOUR) {
    const minutes = Math.floor(delta / MINUTE);
    return minutes === 1 ? 'לפני דקה' : `לפני ${minutes} דקות`;
  }
  if (delta < DAY) {
    const hours = Math.floor(delta / HOUR);
    return hours === 1 ? 'לפני שעה' : `לפני ${hours} שעות`;
  }
  if (delta < 7 * DAY) {
    const days = Math.floor(delta / DAY);
    return days === 1 ? 'אתמול' : `לפני ${days} ימים`;
  }
  return formatDate(value);
}
