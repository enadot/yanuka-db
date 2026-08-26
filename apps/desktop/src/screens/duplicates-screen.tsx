import { useState } from 'react';
import { Link } from 'react-router-dom';
import { ArrowRight, Copy, UserCheck } from 'lucide-react';
import { toast } from 'sonner';
import { initials, type DuplicatePair } from '@yanuka/core';
import type { ContactSummary, Ulid } from '@yanuka/types';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  Badge,
  Button,
  Card,
  CardContent,
  ContactAvatar,
  EmptyState,
  Skeleton,
  TagPill,
} from '@yanuka/ui';
import { useDuplicatePairs, useMergeContacts } from '../hooks/use-contacts';

/**
 * Review and merge likely duplicates.
 *
 * One pair, one decision. Each side shows enough to tell two people apart
 * (name, profession, city, primary phone, tags), and the action names its
 * direction — "לשמור את זה" on the side that survives. The merge itself
 * loses nothing: children move over, conflicting fields land in the notes,
 * and the merged record stays in the mutation log. Still, a merge is a
 * judgement call, so it is confirmed before it runs and never suggested as
 * anything stronger than a candidate.
 */
export function DuplicatesScreen() {
  const { data: pairs, isLoading } = useDuplicatePairs();
  const merge = useMergeContacts();
  const [dismissed, setDismissed] = useState<Set<string>>(new Set());
  const [pendingMerge, setPendingMerge] = useState<{
    keep: ContactSummary;
    merge: ContactSummary;
  } | null>(null);

  const pairKey = (pair: DuplicatePair) => `${pair.first.id}:${pair.second.id}`;
  const visible = (pairs ?? []).filter((pair) => !dismissed.has(pairKey(pair)));

  const runMerge = async (keepId: Ulid, mergeId: Ulid) => {
    try {
      const kept = await merge.mutateAsync({ keepId, mergeId });
      toast.success(`מוזג אל ${kept.displayName}`);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : 'המיזוג נכשל');
    } finally {
      setPendingMerge(null);
    }
  };

  return (
    <div className="mx-auto max-w-4xl space-y-6 p-6">
      <div className="flex items-center gap-3">
        <Button asChild variant="ghost" size="icon" aria-label="חזרה להגדרות">
          <Link to="/settings">
            <ArrowRight className="size-4" aria-hidden />
          </Link>
        </Button>
        <h1 className="text-xl font-semibold">כפילויות אפשריות</h1>
        {pairs && pairs.length > 0 ? (
          <Badge variant="secondary" className="numeric">
            {visible.length}
          </Badge>
        ) : null}
      </div>

      {isLoading ? (
        <div className="space-y-4">
          <Skeleton className="h-40 w-full" />
          <Skeleton className="h-40 w-full" />
        </div>
      ) : visible.length === 0 ? (
        <EmptyState
          icon={<Copy className="size-8" aria-hidden />}
          title="לא נמצאו כפילויות"
          description="הסריקה משווה מספרי טלפון, כתובות אימייל ושמות בכל המאגר."
        />
      ) : (
        visible.map((pair) => (
          <Card key={pairKey(pair)} data-testid="duplicate-pair">
            <CardContent className="space-y-4 pt-6">
              <div className="flex flex-wrap items-center gap-2">
                {pair.reasons.map((reason) => (
                  <Badge key={reason} variant="secondary">
                    {reason}
                  </Badge>
                ))}
              </div>
              <div className="grid gap-4 sm:grid-cols-2">
                <PairSide
                  contact={pair.first}
                  onKeep={() => setPendingMerge({ keep: pair.first, merge: pair.second })}
                />
                <PairSide
                  contact={pair.second}
                  onKeep={() => setPendingMerge({ keep: pair.second, merge: pair.first })}
                />
              </div>
              <Button
                variant="ghost"
                size="sm"
                className="text-muted-foreground"
                onClick={() => setDismissed((current) => new Set(current).add(pairKey(pair)))}
              >
                אלו אנשים שונים — הסתר
              </Button>
            </CardContent>
          </Card>
        ))
      )}

      <AlertDialog open={pendingMerge !== null} onOpenChange={(open) => !open && setPendingMerge(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>למזג את שני אנשי הקשר?</AlertDialogTitle>
            <AlertDialogDescription>
              {pendingMerge
                ? `„${pendingMerge.merge.displayName}" ימוזג אל „${pendingMerge.keep.displayName}". ` +
                  'שום מידע לא נמחק: טלפונים, הערות וקשרים עוברים, ושדות סותרים נשמרים בהערות.'
                : ''}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>ביטול</AlertDialogCancel>
            <AlertDialogAction
              data-testid="confirm-merge"
              disabled={merge.isPending}
              onClick={() => {
                if (pendingMerge) {
                  void runMerge(pendingMerge.keep.id, pendingMerge.merge.id);
                }
              }}
            >
              מיזוג
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

function PairSide({ contact, onKeep }: { contact: ContactSummary; onKeep: () => void }) {
  return (
    <div className="flex flex-col gap-3 rounded-lg border p-4">
      <div className="flex items-center gap-3">
        <ContactAvatar name={contact.displayName} initials={initials(contact.displayName)} />
        <div className="min-w-0">
          <Link
            to={`/contacts/${contact.id}`}
            className="block truncate font-medium text-primary hover:underline"
          >
            {contact.displayName}
          </Link>
          <p className="truncate text-sm text-muted-foreground">
            {[contact.profession, contact.city].filter(Boolean).join(' · ') || '—'}
          </p>
        </div>
      </div>
      {contact.primaryPhone ? (
        <p className="ltr-inline text-sm text-muted-foreground">{contact.primaryPhone}</p>
      ) : null}
      {contact.tags.length > 0 ? (
        <div className="flex flex-wrap gap-1.5">
          {contact.tags.map((tag) => (
            <TagPill key={tag} name={tag} />
          ))}
        </div>
      ) : null}
      <Button
        variant="outline"
        size="sm"
        className="mt-auto"
        aria-label={`לשמור את ${contact.displayName}`}
        onClick={onKeep}
      >
        <UserCheck className="size-4" aria-hidden />
        לשמור את זה
      </Button>
    </div>
  );
}
