import { Link } from 'react-router-dom';
import { ArrowRight, RotateCcw, Trash2 } from 'lucide-react';
import { toast } from 'sonner';
import { initials } from '@yanuka/core';
import type { Ulid } from '@yanuka/types';
import { Badge, Button, Card, CardContent, ContactAvatar, EmptyState, Skeleton } from '@yanuka/ui';
import { countryName, formatDateTime } from '@yanuka/utils';
import { useDeletedContacts, useRestoreContact } from '../hooks/use-contacts';

/**
 * The recycle bin.
 *
 * Deletion here has always been soft — the row survives so the change can sync
 * and be undone — and the toast shown after deleting says so out loud. But
 * every list and every search reads `deleted_at IS NULL`, so once that toast
 * faded the record was unreachable: a soft delete nothing can list is a hard
 * delete with extra steps, on the one product whose first priority is that
 * information does not get lost.
 *
 * There is no "empty the bin". See docs/DECISIONS.md ADR-031: a permanent
 * delete is the only operation here that actually destroys something, and it
 * would run precisely when the user is annoyed and in a hurry.
 */
export function TrashScreen() {
  const { data: deleted, isLoading } = useDeletedContacts();
  const restore = useRestoreContact();

  const restoreContact = async (id: Ulid, name: string) => {
    try {
      await restore.mutateAsync(id);
      toast.success(`${name} שוחזר`);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : 'השחזור נכשל');
    }
  };

  return (
    <div className="mx-auto max-w-4xl space-y-4 p-6">
      <Button asChild variant="ghost" size="sm" className="-ms-2 gap-1">
        <Link to="/settings">
          <ArrowRight className="size-4" aria-hidden />
          חזרה להגדרות
        </Link>
      </Button>

      <div>
        <h1 className="text-xl font-semibold">סל המחזור</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          אנשי קשר שנמחקו. שום דבר לא נמחק לצמיתות — כל רשומה כאן ניתנת לשחזור מלא, על כל הטלפונים,
          ההערות והקשרים שלה.
        </p>
      </div>

      {isLoading ? (
        <div className="space-y-2">
          <Skeleton className="h-20 w-full" />
          <Skeleton className="h-20 w-full" />
        </div>
      ) : null}

      {!isLoading && (deleted ?? []).length === 0 ? (
        <EmptyState
          icon={<Trash2 className="size-8" aria-hidden />}
          title="סל המחזור ריק"
          description="לא נמחקו אנשי קשר."
          action={
            <Button asChild variant="outline">
              <Link to="/contacts">לרשימת אנשי הקשר</Link>
            </Button>
          }
        />
      ) : null}

      <div className="space-y-2">
        {(deleted ?? []).map(({ contact, deletedAt }) => (
          <Card key={contact.id} data-testid="deleted-contact">
            <CardContent className="flex items-center gap-4 p-4">
              <ContactAvatar name={contact.displayName} initials={initials(contact.displayName)} />

              <div className="min-w-0 flex-1">
                <p className="truncate font-medium">{contact.displayName}</p>
                <p className="truncate text-sm text-muted-foreground">
                  {[contact.profession, contact.city, countryName(contact.country)]
                    .filter(Boolean)
                    .join(' · ') || 'ללא פרטים נוספים'}
                </p>
                {contact.primaryPhone ? (
                  <p className="numeric ltr-inline text-sm text-muted-foreground">
                    {contact.primaryPhone}
                  </p>
                ) : null}
              </div>

              <Badge variant="secondary" className="font-normal">
                נמחק {formatDateTime(deletedAt)}
              </Badge>

              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={restore.isPending}
                onClick={() => void restoreContact(contact.id, contact.displayName)}
              >
                <RotateCcw className="size-4" aria-hidden />
                שחזור
              </Button>
            </CardContent>
          </Card>
        ))}
      </div>
    </div>
  );
}
