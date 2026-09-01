import { useState } from 'react';
import { Link } from 'react-router-dom';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { ArrowRight, Check, GitCompareArrows, Laptop, Smartphone } from 'lucide-react';
import { toast } from 'sonner';
import { formatDateTime, formatRelative } from '@yanuka/utils';
import { Badge, Button, Card, CardContent, EmptyState, Separator, Skeleton } from '@yanuka/ui';
import {
  openConflicts,
  resolveConflict,
  type ConflictSide,
  type FieldConflict,
  type OpenConflict,
} from '../lib/desktop-io';
import { fieldLabel, formatFieldValue } from '../lib/field-labels';

/**
 * Choosing between two answers to the same question.
 *
 * This screen exists because the merge refuses to choose. When two devices
 * both edit one field while offline, `apply` keeps what is here, records what
 * arrived, and asks — and until now the only way to answer was to open the
 * contact and retype the winning value by hand, which meant reading a number
 * off one screen and typing it into another. That is exactly the operation a
 * person gets wrong.
 *
 * Three things shape how it is written:
 *
 * * **Nothing is preselected.** A default here is a decision made on the
 *   user's behalf about data they typed, and one careless click would confirm
 *   it. The save button stays disabled until something is actually chosen.
 *
 * * **The sides are named by place, not by device id.** "במחשב הזה" is
 *   something a person can reason about; `01JQ7…` is not, and it is the same
 *   information.
 *
 * * **A field can be left alone.** Not every disagreement can be settled at the
 *   moment it is noticed — some need a phone call. Deciding one field and
 *   leaving the other open is a supported outcome, not an abandoned form.
 */
export function ConflictsScreen() {
  const queryClient = useQueryClient();
  const { data: conflicts, isLoading } = useQuery({
    queryKey: ['conflicts'],
    queryFn: openConflicts,
  });

  return (
    <div className="mx-auto max-w-4xl space-y-6 p-6">
      <div className="flex items-center gap-3">
        <Button asChild variant="ghost" size="icon" aria-label="חזרה להגדרות">
          <Link to="/settings">
            <ArrowRight className="size-4" aria-hidden />
          </Link>
        </Button>
        <h1 className="text-xl font-semibold">שינויים בשתי גרסאות</h1>
        {conflicts && conflicts.length > 0 ? (
          <Badge variant="secondary" className="numeric">
            {conflicts.length}
          </Badge>
        ) : null}
      </div>

      {isLoading ? (
        <div className="space-y-4">
          <Skeleton className="h-48 w-full" />
          <Skeleton className="h-48 w-full" />
        </div>
      ) : !conflicts || conflicts.length === 0 ? (
        <EmptyState
          icon={<GitCompareArrows className="size-8" aria-hidden />}
          title="אין מה להכריע"
          description="כששני מכשירים משנים את אותו שדה בזמן שאין ביניהם חיבור, שתי הגרסאות נשמרות והשאלה מופיעה כאן."
        />
      ) : (
        <>
          <p className="text-sm text-muted-foreground">
            אותו פרט נכתב אחרת בשני מכשירים, ולכן אף גרסה לא נמחקה. הגרסה שתיבחר תחליף את השנייה
            בכל המכשירים; מה שלא ייבחר עכשיו יישאר כאן.
          </p>
          {conflicts.map((conflict) => (
            <ConflictCard
              key={conflict.id}
              conflict={conflict}
              onResolved={() => queryClient.invalidateQueries()}
            />
          ))}
        </>
      )}
    </div>
  );
}

function ConflictCard({
  conflict,
  onResolved,
}: {
  conflict: OpenConflict;
  onResolved: () => void;
}) {
  const [choices, setChoices] = useState<Record<string, ConflictSide>>({});

  const save = useMutation({
    mutationFn: () =>
      resolveConflict(
        conflict.id,
        Object.entries(choices).map(([field, side]) => ({ field, side })),
      ),
    onSuccess: () => {
      setChoices({});
      toast.success('הבחירה נשמרה ותישלח לשאר המכשירים');
      onResolved();
    },
    onError: (error: unknown) =>
      toast.error(error instanceof Error ? error.message : 'שמירת הבחירה נכשלה'),
  });

  const chosen = Object.keys(choices).length;

  return (
    <Card data-testid="conflict">
      <CardContent className="space-y-4 pt-6">
        <div className="flex flex-wrap items-baseline justify-between gap-2">
          {conflict.displayName ? (
            <Link
              to={`/contacts/${conflict.entityId}`}
              className="font-medium text-primary hover:underline"
            >
              {conflict.displayName}
            </Link>
          ) : (
            <span className="font-medium text-muted-foreground">איש קשר שנמחק</span>
          )}
          <span className="text-xs text-muted-foreground">
            התגלה {formatRelative(conflict.detectedAt)}
          </span>
        </div>

        {conflict.fields.map((field, index) => (
          <div key={field.field} className="space-y-2">
            {index > 0 ? <Separator /> : null}
            <p className="text-sm font-medium">{fieldLabel(field.field)}</p>
            <div className="grid gap-3 sm:grid-cols-2">
              <SideOption
                field={field}
                side="local"
                selected={choices[field.field] === 'local'}
                onSelect={() =>
                  setChoices((current) => ({ ...current, [field.field]: 'local' }))
                }
              />
              <SideOption
                field={field}
                side="remote"
                selected={choices[field.field] === 'remote'}
                onSelect={() =>
                  setChoices((current) => ({ ...current, [field.field]: 'remote' }))
                }
              />
            </div>
          </div>
        ))}

        <div className="flex items-center justify-between gap-4">
          <p className="text-xs text-muted-foreground">
            {chosen === 0
              ? 'יש לבחור גרסה כדי לשמור.'
              : chosen < conflict.fields.length
                ? `נבחרו ${chosen} מתוך ${conflict.fields.length}. השאר יישאר פתוח.`
                : 'נבחרו כל השדות.'}
          </p>
          <Button
            type="button"
            data-testid="save-conflict-choice"
            disabled={chosen === 0 || save.isPending}
            onClick={() => save.mutate()}
          >
            <Check className="size-4" aria-hidden />
            {save.isPending ? 'שומר…' : 'שמירת הבחירה'}
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

function SideOption({
  field,
  side,
  selected,
  onSelect,
}: {
  field: FieldConflict;
  side: ConflictSide;
  selected: boolean;
  onSelect: () => void;
}) {
  const isLocal = side === 'local';
  const value = formatFieldValue(field.field, isLocal ? field.localValue : field.remoteValue);
  const at = isLocal ? field.localUpdatedAt : field.remoteUpdatedAt;
  const Icon = isLocal ? Laptop : Smartphone;
  const where = isLocal ? 'במחשב הזה' : 'הגיע ממכשיר אחר';

  return (
    <button
      type="button"
      aria-pressed={selected}
      onClick={onSelect}
      className={`flex flex-col items-start gap-2 rounded-lg border p-4 text-start transition-colors ${
        selected ? 'border-primary bg-primary/5 ring-1 ring-primary' : 'hover:bg-muted/50'
      }`}
    >
      <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
        <Icon className="size-3.5" aria-hidden />
        {where}
      </span>
      {/* An empty value is a real answer — the field was cleared on that side —
          and has to look different from an answer that is merely short. */}
      <span className={value ? 'break-words text-sm' : 'text-sm italic text-muted-foreground'}>
        {value || 'ריק'}
      </span>
      <span className="text-xs text-muted-foreground">{formatDateTime(at)}</span>
    </button>
  );
}
