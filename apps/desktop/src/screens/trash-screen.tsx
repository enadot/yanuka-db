import { ArchiveRestore, Trash2 } from 'lucide-react';
import { Link } from 'react-router-dom';
import { toast } from 'sonner';
import { initials } from '@yanuka/core';
import { formatDateTime } from '@yanuka/utils';
import { Button, Card, CardContent, ContactAvatar, EmptyState, Skeleton } from '@yanuka/ui';
import { useDeletedContacts, useRestoreContact } from '../hooks/use-contacts';

/**
 * The recycle bin.
 *
 * Deletion here is soft — the row survives with a deletion timestamp — and
 * this screen is what makes that visible. Without it, the only undo was the
 * few seconds a toast stays on screen, which is not a promise this product
 * is allowed to make about its own priority 1 (מידע לא הולך לאיבוד).
 *
 * There is intentionally no "empty the trash" button: permanent erasure is a
 * sync-era decision (the deletion has to propagate first), and offering it
 * now would contradict the reason this screen exists.
 */
export function TrashScreen() {
  const { data: deleted, isLoading } = useDeletedContacts();
  const restore = useRestoreContact();

  const restoreOne = async (id: string, name: string) => {
    await restore.mutateAsync(id);
    toast.success(`${name} שוחזר אל המאגר`);
  };

  return (
    <div className="mx-auto max-w-3xl space-y-6 p-6">
      <header className="space-y-1">
        <h1 className="flex items-center gap-2 text-xl font-semibold">
          <Trash2 className="size-5" aria-hidden />
          סל המחזור
        </h1>
        <p className="text-sm text-muted-foreground">
          אנשי קשר שנמחקו נשארים כאן וניתנים לשחזור מלא — על כל ההערות, הקשרים והפרטים שלהם.
        </p>
      </header>

      {isLoading ? (
        <div className="space-y-2">
          <Skeleton className="h-16 w-full" />
          <Skeleton className="h-16 w-full" />
        </div>
      ) : !deleted || deleted.length === 0 ? (
        <EmptyState
          icon={<Trash2 className="size-8" aria-hidden />}
          title="סל המחזור ריק"
          description="כשאיש קשר נמחק הוא יופיע כאן, וניתן יהיה להחזיר אותו בלחיצה."
          action={
            <Button asChild variant="outline">
              <Link to="/contacts">לרשימת אנשי הקשר</Link>
            </Button>
          }
        />
      ) : (
        <div className="space-y-2">
          {deleted.map((contact) => (
            <Card key={contact.id} data-testid="trash-row">
              <CardContent className="flex items-center gap-4 p-4">
                <ContactAvatar name={contact.displayName} initials={initials(contact.displayName)} />
                <div className="min-w-0 flex-1">
                  <p className="truncate font-medium">{contact.displayName}</p>
                  <p className="text-xs text-muted-foreground">
                    {[contact.profession, contact.city].filter(Boolean).join(' · ') || '—'}
                  </p>
                  <p className="text-xs text-muted-foreground">
                    נמחק: {formatDateTime(contact.deletedAt)}
                  </p>
                </div>
                <Button
                  variant="outline"
                  className="shrink-0 gap-1.5"
                  onClick={() => void restoreOne(contact.id, contact.displayName)}
                  disabled={restore.isPending}
                  data-testid="restore-contact"
                  aria-label={`שחזור ${contact.displayName}`}
                >
                  <ArchiveRestore className="size-4" aria-hidden />
                  שחזור
                </Button>
              </CardContent>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}
