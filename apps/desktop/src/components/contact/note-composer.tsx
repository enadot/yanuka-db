import { useState } from 'react';
import { Check, Pencil, Plus, Trash2, X } from 'lucide-react';
import { toast } from 'sonner';
import type { ContactWithRelations } from '@yanuka/types';
import { Button, Card, CardContent, CardHeader, CardTitle, Textarea } from '@yanuka/ui';
import { formatDateTime } from '@yanuka/utils';
import { useAddNote, useDeleteNote, useUpdateNote } from '../../hooks/use-contacts';

export interface NoteComposerProps {
  contact: ContactWithRelations;
}

/**
 * Timestamped notes, added from the card itself.
 *
 * Distinct from `contacts.notes`, which is the one always-visible remark about
 * who the person is. These are dated entries — what was said in a conversation,
 * what was asked for, who else came up — and they accumulate. Keeping them
 * separate means a new entry never has to be pasted into the end of an existing
 * paragraph, which is how the date of the original remark gets lost.
 *
 * They are searchable like every other note, so a sentence written here answers
 * the question the archive exists for.
 */
export function NoteComposer({ contact }: NoteComposerProps) {
  const [draft, setDraft] = useState('');
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editingBody, setEditingBody] = useState('');

  const addNote = useAddNote();
  const updateNote = useUpdateNote();
  const deleteNote = useDeleteNote();

  const submit = async () => {
    const body = draft.trim();
    if (!body) return;
    try {
      await addNote.mutateAsync({ contactId: contact.id, body, isSensitive: false });
      setDraft('');
      toast.success('ההערה נוספה');
    } catch {
      toast.error('הוספת ההערה נכשלה');
    }
  };

  const saveEdit = async () => {
    if (!editingId) return;
    const body = editingBody.trim();
    if (!body) return;
    try {
      await updateNote.mutateAsync({ id: editingId, body });
      setEditingId(null);
      toast.success('ההערה עודכנה');
    } catch {
      toast.error('עדכון ההערה נכשל');
    }
  };

  const remove = async (id: string) => {
    try {
      await deleteNote.mutateAsync(id);
      toast.success('ההערה נמחקה');
    } catch {
      toast.error('מחיקת ההערה נכשלה');
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">יומן הערות</CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        {contact.contactNotes.map((note) => (
          <div key={note.id} className="rounded-md border p-3">
            {editingId === note.id ? (
              <div className="space-y-2">
                <Textarea
                  rows={3}
                  aria-label="עריכת ההערה"
                  value={editingBody}
                  onChange={(event) => setEditingBody(event.target.value)}
                />
                <div className="flex gap-2">
                  <Button
                    type="button"
                    size="sm"
                    disabled={updateNote.isPending}
                    onClick={() => void saveEdit()}
                  >
                    <Check className="size-4" aria-hidden />
                    שמירה
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    variant="ghost"
                    onClick={() => setEditingId(null)}
                  >
                    <X className="size-4" aria-hidden />
                    ביטול
                  </Button>
                </div>
              </div>
            ) : (
              <>
                <p className="whitespace-pre-wrap text-sm">{note.body}</p>
                <div className="mt-2 flex items-center gap-2">
                  <span className="text-xs text-muted-foreground">
                    {formatDateTime(note.createdAt)}
                  </span>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    className="ms-auto"
                    aria-label="עריכת ההערה"
                    onClick={() => {
                      setEditingId(note.id);
                      setEditingBody(note.body);
                    }}
                  >
                    <Pencil className="size-4" />
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    aria-label="מחיקת ההערה"
                    onClick={() => void remove(note.id)}
                  >
                    <Trash2 className="size-4 text-destructive" />
                  </Button>
                </div>
              </>
            )}
          </div>
        ))}

        <Textarea
          rows={3}
          aria-label="הערה חדשה"
          placeholder="מה נאמר, מה ביקשו, ומי עוד עלה בשיחה"
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
        />
        <Button
          type="button"
          size="sm"
          disabled={!draft.trim() || addNote.isPending}
          onClick={() => void submit()}
        >
          <Plus className="size-4" aria-hidden />
          הוספת הערה
        </Button>
      </CardContent>
    </Card>
  );
}
