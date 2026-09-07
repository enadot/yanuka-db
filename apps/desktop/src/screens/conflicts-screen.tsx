import { Check, GitMerge, Laptop, Pencil, TriangleAlert } from 'lucide-react';
import { Link } from 'react-router-dom';
import { toast } from 'sonner';
import type { ConflictView } from '@yanuka/types';
import { formatDateTime } from '@yanuka/utils';
import { Button, Card, CardContent, CardHeader, CardTitle, EmptyState, Skeleton } from '@yanuka/ui';
import {
  useCategories,
  useConflicts,
  useResolveConflict,
  useTags,
} from '../hooks/use-contacts';
import { FIELD_LABELS, valueText } from '../lib/field-text';

/**
 * Fields two devices changed differently (SYNC.md §Conflicts).
 *
 * Both values are on the screen, side by side, with the device each came
 * from. Nothing was chosen for the person and nothing will be: the record
 * stays out of sync until they decide here — or edit it by hand and say so.
 */
export function ConflictsScreen() {
  const { data: conflicts, isLoading } = useConflicts();
  const resolve = useResolveConflict();
  const { data: tags = [] } = useTags();
  const { data: categories = [] } = useCategories();
  const lookup = (id: string) =>
    tags.find((tag) => tag.id === id)?.name ?? categories.find((c) => c.id === id)?.name;

  const settle = async (conflict: ConflictView, resolution: 'local' | 'remote' | 'manual') => {
    await resolve.mutateAsync({ id: conflict.id, resolution });
    toast.success(
      resolution === 'local'
        ? 'הגרסה שלך נשמרה ותישלח לשאר המכשירים'
        : resolution === 'remote'
          ? 'הגרסה מהמכשיר האחר נשמרה כאן'
          : 'ההתנגשות נסגרה; הרשומה כפי שערכת תישלח',
    );
  };

  return (
    <div className="mx-auto max-w-3xl space-y-6 p-6">
      <header className="space-y-1">
        <h1 className="flex items-center gap-2 text-xl font-semibold">
          <GitMerge className="size-5" aria-hidden />
          התנגשויות סנכרון
        </h1>
        <p className="text-sm text-muted-foreground">
          שני מכשירים שינו את אותו פרט בדרכים שונות. שתי הגרסאות נשמרו ואף אחת לא נמחקה; בחרו
          איזו נכונה, או ערכו את הרשומה ידנית.
        </p>
      </header>

      {isLoading ? (
        <div className="space-y-2">
          <Skeleton className="h-24 w-full" />
        </div>
      ) : !conflicts || conflicts.length === 0 ? (
        <EmptyState
          icon={<Check className="size-8" aria-hidden />}
          title="אין התנגשויות"
          description="כל המכשירים מסכימים על כל הרשומות."
          action={
            <Button asChild variant="outline">
              <Link to="/settings">להגדרות הסנכרון</Link>
            </Button>
          }
        />
      ) : (
        <div className="space-y-3">
          {conflicts.map((conflict) => {
            const remote = conflict.remoteDeviceName ?? 'מכשיר אחר';
            const editHref =
              conflict.entityType === 'contact' ? `/contacts/${conflict.entityId}/edit` : null;
            return (
              <Card key={conflict.id} data-testid="conflict-row">
                <CardHeader>
                  <CardTitle className="flex items-baseline justify-between gap-3 text-base">
                    <span className="flex items-center gap-2">
                      <TriangleAlert className="size-4 text-amber-600" aria-hidden />
                      {conflict.entityType === 'contact' ? (
                        <Link to={`/contacts/${conflict.entityId}`} className="hover:underline">
                          {conflict.entityLabel ?? 'רשומה'}
                        </Link>
                      ) : (
                        <span>{conflict.entityLabel ?? ENTITY_LABELS[conflict.entityType]}</span>
                      )}
                      <span className="text-xs font-normal text-muted-foreground">
                        {ENTITY_LABELS[conflict.entityType]}
                      </span>
                    </span>
                    <span className="shrink-0 text-xs font-normal text-muted-foreground">
                      זוהה {formatDateTime(conflict.detectedAt)}
                    </span>
                  </CardTitle>
                </CardHeader>
                <CardContent className="space-y-4">
                  <div className="space-y-3">
                    {conflict.fields.map((field) => (
                      <div key={field.field} className="space-y-1.5">
                        <p className="text-sm font-medium">{FIELD_LABELS[field.field] ?? field.field}</p>
                        <div className="grid gap-2 sm:grid-cols-2">
                          <Side
                            title="במחשב הזה"
                            value={valueText(field.localValue, { lookup })}
                            testId="conflict-local-value"
                          />
                          <Side
                            title={`ב${remote}`}
                            value={valueText(field.remoteValue, { lookup })}
                            testId="conflict-remote-value"
                          />
                        </div>
                      </div>
                    ))}
                  </div>
                  <div className="flex flex-wrap gap-2">
                    <Button
                      onClick={() => void settle(conflict, 'local')}
                      disabled={resolve.isPending}
                      data-testid="conflict-local"
                    >
                      <Laptop className="size-4" aria-hidden />
                      לשמור את הגרסה שלי
                    </Button>
                    <Button
                      variant="outline"
                      onClick={() => void settle(conflict, 'remote')}
                      disabled={resolve.isPending}
                      data-testid="conflict-remote"
                    >
                      לקחת את הגרסה מ{remote}
                    </Button>
                    {editHref ? (
                      <Button asChild variant="ghost">
                        <Link to={editHref}>
                          <Pencil className="size-4" aria-hidden />
                          לערוך ידנית
                        </Link>
                      </Button>
                    ) : null}
                    <Button
                      variant="ghost"
                      onClick={() => void settle(conflict, 'manual')}
                      disabled={resolve.isPending}
                      data-testid="conflict-manual"
                    >
                      ערכתי ידנית — לסגור
                    </Button>
                  </div>
                </CardContent>
              </Card>
            );
          })}
        </div>
      )}
    </div>
  );
}

const ENTITY_LABELS: Record<string, string> = {
  contact: 'איש קשר',
  note: 'הערה',
  tag: 'תגית',
  category: 'קטגוריה',
  organization: 'מוסד',
  relationship: 'קשר',
  contact_phone: 'טלפון',
  contact_email: 'אימייל',
  contact_alias: 'כינוי',
  contact_tag: 'תגית',
  contact_category: 'קטגוריה',
  contact_organization: 'מוסד',
};

function Side({ title, value, testId }: { title: string; value: string; testId: string }) {
  return (
    <div className="rounded-md border bg-muted/30 p-2.5">
      <p className="text-xs text-muted-foreground">{title}</p>
      <p className="mt-0.5 break-words text-sm" data-testid={testId}>
        {value}
      </p>
    </div>
  );
}
