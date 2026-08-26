/**
 * CSV export — the mirror of import, and a portability guarantee.
 *
 * The headers are the Hebrew names `suggestMapping` recognizes, so a file
 * exported here re-imports with the mapping already correct. That closed loop
 * is the point: the archive can leave this application intact at any moment,
 * into a format Excel opens and the importer round-trips. What the CSV cannot
 * carry (relationship edges, organization links, timestamped note metadata)
 * lives in the database backups; this is the human-readable snapshot.
 */

import type { ContactWithRelations } from '@yanuka/types';

const BOM = '﻿';
const SEPARATOR = ',';
const LINE = '\r\n';

function escapeField(value: string): string {
  if (value.includes(SEPARATOR) || value.includes('"') || value.includes('\n')) {
    return `"${value.replaceAll('"', '""')}"`;
  }
  return value;
}

/**
 * Serialize contacts to CSV.
 *
 * Phone and email columns repeat as many times as the widest contact needs
 * (`טלפון 1`, `טלפון 2`, …) — both header forms contain the keyword the
 * import auto-detection matches on. The BOM is for Excel, which otherwise
 * guesses a legacy encoding and mangles the Hebrew; our own parser strips it.
 */
export function buildContactsCsv(contacts: ContactWithRelations[]): string {
  const phoneColumns = Math.max(1, ...contacts.map((contact) => contact.phones.length));
  const emailColumns = Math.max(1, ...contacts.map((contact) => contact.emails.length));

  const headers = [
    'שם מלא',
    'שם פרטי',
    'שם משפחה',
    'תואר',
    ...Array.from({ length: phoneColumns }, (_, i) => `טלפון ${i + 1}`),
    ...Array.from({ length: emailColumns }, (_, i) => `אימייל ${i + 1}`),
    'עיר',
    'כתובת',
    'מקצוע',
    'תפקיד',
    'הערות',
    'נשמר בגלל',
    'מקור',
    'הכיר בינינו',
  ];

  const rows = contacts.map((contact) => {
    const phones = Array.from(
      { length: phoneColumns },
      (_, i) => contact.phones[i]?.raw ?? '',
    );
    const emails = Array.from(
      { length: emailColumns },
      (_, i) => contact.emails[i]?.address ?? '',
    );
    return [
      contact.displayName,
      contact.firstName ?? '',
      contact.lastName ?? '',
      contact.prefix ?? '',
      ...phones,
      ...emails,
      contact.city ?? '',
      contact.address ?? '',
      contact.profession ?? '',
      contact.role ?? '',
      contact.notes ?? '',
      contact.reasonForSaving ?? '',
      contact.source ?? '',
      contact.introducedBy ?? '',
    ];
  });

  return (
    BOM +
    [headers, ...rows].map((row) => row.map(escapeField).join(SEPARATOR)).join(LINE) +
    LINE
  );
}
