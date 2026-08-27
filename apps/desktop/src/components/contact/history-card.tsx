import { useState } from 'react';
import { History } from 'lucide-react';
import type { AuditLogEntry } from '@yanuka/types';
import { formatDateTime } from '@yanuka/utils';
import { Button, Card, CardContent, CardHeader, CardTitle, Separator } from '@yanuka/ui';
import { useContactHistory } from '../../hooks/use-contacts';

/**
 * What happened to this record, straight from the mutation journal.
 *
 * Every write already lands in the journal with the fields it changed and the
 * values they replaced (that is the sync design working for the user before
 * any server exists). An accidental edit that overwrote a note written years
 * ago is recoverable by reading, not by guessing.
 */

/** The same wording the edit form uses, so history reads like the form. */
const FIELD_LABELS: Record<string, string> = {
  displayName: 'שם מלא',
  firstName: 'שם פרטי',
  lastName: 'שם משפחה',
  prefix: 'תואר',
  title: 'תפקיד',
  country: 'מדינה',
  region: 'אזור',
  city: 'עיר',
  address: 'כתובת',
  postalCode: 'מיקוד',
  profession: 'מקצוע',
  role: 'תפקיד',
  notes: 'הערה חופשית',
  reasonForSaving: 'נשמר בגלל',
  source: 'מקור',
  introducedBy: 'מי הכיר',
};

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

function asText(value: unknown): string {
  if (value === null || value === undefined || value === '') return 'ריק';
  return String(value);
}

const INITIAL_COUNT = 5;

export function HistoryCard({ contactId }: { contactId: string }) {
  const { data: entries = [] } = useContactHistory(contactId);
  const [expanded, setExpanded] = useState(false);

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
              <p className="text-sm font-medium">{ACTION_LABELS[entry.action] ?? entry.action}</p>
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
