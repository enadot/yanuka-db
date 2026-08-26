import { useState } from 'react';
import { Pencil, Trash2 } from 'lucide-react';
import type { ContactWithRelations } from '@yanuka/types';
import { formatDateTime } from '@yanuka/utils';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Textarea,
} from '@yanuka/ui';
import { toast } from 'sonner';
import { useAddNote, useDeleteNote, useUpdateNote } from '../../hooks/use-contacts';

/**
 * The notes on one contact — read, add, edit, delete.
 *
 * These timestamped notes are where the archive's value accrues between edits
 * of the contact itself: "פגשתי אותו אצל אדלר, לחזור אליו בעניין הספרייה".
 * Every write reindexes the contact, so the phrase becomes findable the moment
 * it is saved — which is the product's core promise, and why the empty state
 * says so instead of just sitting there.
 */
export function NotesCard({ contact }: { contact: ContactWithRelations }) {
  const addNote = useAddNote();
  const updateNote = useUpdateNote();
  const deleteNote = useDeleteNote();

  const [draft, setDraft] = useState('');
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editBody, setEditBody] = useState('');

  const submit = async () => {
    try {
      await addNote.mutateAsync({ contactId: contact.id, body: draft.trim() });
      setDraft('');
      toast.success('ההערה נשמרה ונכנסה לחיפוש');
    } catch (error) {
      toast.error(error instanceof Error ? error.message : 'שמירת ההערה נכשלה');
    }
  };

  const saveEdit = async () => {
    if (!editingId) return;
    try {
      await updateNote.mutateAsync({ id: editingId, body: editBody.trim() });
      setEditingId(null);
      toast.success('ההערה עודכנה');
    } catch (error) {
      toast.error(error instanceof Error ? error.message : 'עדכון ההערה נכשל');
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">הערות</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        {contact.notes ? (
          <p className="whitespace-pre-wrap text-sm leading-relaxed">{contact.notes}</p>
        ) : null}

        {contact.reasonForSaving ? (
          <div className="rounded-md border-s-2 border-s-primary bg-muted/40 p-3">
            <p className="mb-1 text-xs font-medium text-muted-foreground">נשמר בגלל</p>
            <p className="text-sm">{contact.reasonForSaving}</p>
          </div>
        ) : null}

        {contact.contactNotes.map((note) =>
          editingId === note.id ? (
            <div key={note.id} className="space-y-2 rounded-md border p-3">
              <Textarea
                value={editBody}
                onChange={(event) => setEditBody(event.target.value)}
                data-testid="edit-note-body"
                aria-label="תוכן ההערה"
              />
              <div className="flex gap-2">
                <Button
                  size="sm"
                  onClick={() => void saveEdit()}
                  disabled={editBody.trim().length === 0 || updateNote.isPending}
                  data-testid="save-note"
                >
                  שמירה
                </Button>
                <Button size="sm" variant="ghost" onClick={() => setEditingId(null)}>
                  ביטול
                </Button>
              </div>
            </div>
          ) : (
            <div key={note.id} className="rounded-md border p-3" data-testid="contact-note">
              <p className="whitespace-pre-wrap text-sm">{note.body}</p>
              <div className="mt-2 flex items-center justify-between">
                <p className="text-xs text-muted-foreground">{formatDateTime(note.createdAt)}</p>
                <div className="flex gap-1">
                  <Button
                    variant="ghost"
                    size="icon"
                    aria-label="עריכת הערה"
                    onClick={() => {
                      setEditingId(note.id);
                      setEditBody(note.body);
                    }}
                  >
                    <Pencil className="size-3.5" />
                  </Button>
                  <AlertDialog>
                    <AlertDialogTrigger asChild>
                      <Button variant="ghost" size="icon" aria-label="מחיקת הערה">
                        <Trash2 className="size-3.5 text-destructive" />
                      </Button>
                    </AlertDialogTrigger>
                    <AlertDialogContent>
                      <AlertDialogHeader>
                        <AlertDialogTitle>למחוק את ההערה?</AlertDialogTitle>
                        <AlertDialogDescription>
                          ההערה תוסר מהכרטיס ומהחיפוש.
                        </AlertDialogDescription>
                      </AlertDialogHeader>
                      <AlertDialogFooter>
                        <AlertDialogCancel>ביטול</AlertDialogCancel>
                        <AlertDialogAction
                          data-testid="confirm-delete-note"
                          onClick={() =>
                            deleteNote.mutate(note.id, {
                              onSuccess: () => toast.success('ההערה נמחקה'),
                              onError: () => toast.error('מחיקת ההערה נכשלה'),
                            })
                          }
                        >
                          מחיקה
                        </AlertDialogAction>
                      </AlertDialogFooter>
                    </AlertDialogContent>
                  </AlertDialog>
                </div>
              </div>
            </div>
          ),
        )}

        <div className="space-y-2">
          <Textarea
            placeholder="הערה חדשה — מה שנכתב כאן ניתן לחיפוש מיד"
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            data-testid="new-note-body"
            aria-label="הערה חדשה"
          />
          <Button
            onClick={() => void submit()}
            disabled={draft.trim().length === 0 || addNote.isPending}
            data-testid="add-note"
          >
            הוספת הערה
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
