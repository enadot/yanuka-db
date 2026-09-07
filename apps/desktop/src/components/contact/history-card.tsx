import { useState } from 'react';
import { History } from 'lucide-react';
import type { AuditLogEntry } from '@yanuka/types';
import { formatDateTime } from '@yanuka/utils';
import { Button, Card, CardContent, CardHeader, CardTitle, Separator } from '@yanuka/ui';
import { useCategories, useContactHistory, useTags } from '../../hooks/use-contacts';
import { FIELD_LABELS, valueText } from '../../lib/field-text';

/**
 * What happened to this record, straight from the mutation journal.
 *
 * Every write already lands in the journal with the fields it changed and the
 * values they replaced (that is the sync design working for the user before
 * any server exists). An accidental edit that overwrote a note written years
 * ago is recoverable by reading, not by guessing.
 */

const ACTION_LABELS: Record<AuditLogEntry['action'], string> = {
  create: 'הרשומה נוצרה',
  update: 'עודכן',
  delete: 'הועבר לסל המחזור',
  restore: 'שוחזר מסל המחזור',
  merge: 'מוזג עם רשומה כפולה',
  view_sensitive: 'הוצג מידע רגיש',
  export: 'יוצא',
  import: 'יובא',
  login: 'התחברות',
  sync: 'סנכרון',
};

/**
 * Child entries read as their own sentences — "נוספה הערה", not "הרשומה
 * נוצרה" — because on this card the record is the person, and the note or
 * the edge is the thing that happened to them.
 */
const CHILD_ACTION_LABELS: Record<string, Partial<Record<AuditLogEntry['action'], string>>> = {
  note: { create: 'נוספה הערה', update: 'הערה נערכה', delete: 'הערה נמחקה' },
  relationship: { create: 'נרשם קשר', delete: 'קשר הוסר' },
};

function verbFor(entry: AuditLogEntry): string {
  return (
    CHILD_ACTION_LABELS[entry.entityType]?.[entry.action] ??
    ACTION_LABELS[entry.action] ??
    entry.action
  );
}

const INITIAL_COUNT = 5;

export function HistoryCard({ contactId }: { contactId: string }) {
  const { data: entries = [] } = useContactHistory(contactId);
  const { data: tags = [] } = useTags();
  const { data: categories = [] } = useCategories();
  const [expanded, setExpanded] = useState(false);
  // Tag and category edits are journaled by id; the reader wants names.
  const lookup = (id: string) =>
    tags.find((tag) => tag.id === id)?.name ?? categories.find((c) => c.id === id)?.name;
  const asText = (value: unknown) => valueText(value, { maxLength: 80, lookup });

  if (entries.length === 0) return null;

  const visible = expanded ? entries : entries.slice(0, INITIAL_COUNT);

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <History className="size-4" aria-hidden />
          היסטוריה
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        {visible.map((entry, index) => (
          <div key={entry.id} data-testid="history-entry">
            {index > 0 ? <Separator className="mb-3" /> : null}
            <div className="flex items-baseline justify-between gap-3">
              <p className="min-w-0 truncate text-sm font-medium">
                {verbFor(entry)}
                {entry.entityType !== 'contact' && entry.entityLabel ? (
                  <span className="font-normal text-muted-foreground"> · {entry.entityLabel}</span>
                ) : null}
              </p>
              <p className="shrink-0 text-xs text-muted-foreground">
                {formatDateTime(entry.createdAt)}
              </p>
            </div>
            {entry.changes ? (
              <ul className="mt-1 space-y-0.5">
                {Object.entries(entry.changes).map(([field, change]) => (
                  <li key={field} className="text-xs text-muted-foreground">
                    <span className="font-medium text-foreground">
                      {FIELD_LABELS[field] ?? field}
                    </span>
                    : {asText(change.from)} ← {asText(change.to)}
                  </li>
                ))}
              </ul>
            ) : null}
          </div>
        ))}
        {entries.length > INITIAL_COUNT ? (
          <Button
            variant="ghost"
            size="sm"
            className="w-full"
            onClick={() => setExpanded((value) => !value)}
          >
            {expanded ? 'להציג פחות' : `כל ההיסטוריה (${entries.length})`}
          </Button>
        ) : null}
      </CardContent>
    </Card>
  );
}
