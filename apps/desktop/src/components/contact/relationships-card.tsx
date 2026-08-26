import { useState } from 'react';
import { Link } from 'react-router-dom';
import { Plus, Trash2, X } from 'lucide-react';
import {
  RELATIONSHIP_INVERSES,
  RELATIONSHIP_TYPES,
  type ContactWithRelations,
  type RelationshipType,
  type Ulid,
} from '@yanuka/types';
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
  Input,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@yanuka/ui';
import { toast } from 'sonner';
import {
  useCreateRelationship,
  useDeleteRelationship,
  useSuggestions,
} from '../../hooks/use-contacts';

/** How each relationship reads when shown from this contact's side. */
export const RELATIONSHIP_LABELS: Record<RelationshipType, { out: string; in: string }> = {
  recommended: { out: 'המליץ על', in: 'הומלץ על ידי' },
  knows: { out: 'מכיר את', in: 'מוכר ל' },
  related_to: { out: 'קשור ל', in: 'קשור ל' },
  works_with: { out: 'עובד עם', in: 'עובד עם' },
  family_of: { out: 'בן משפחה של', in: 'בן משפחה של' },
  referred_us_to: { out: 'הפנה אותנו ל', in: 'הופנינו אליו על ידי' },
  member_of: { out: 'שייך ל', in: 'כולל את' },
  student_of: { out: 'תלמיד של', in: 'רבו של' },
  teacher_of: { out: 'רבו של', in: 'תלמיד של' },
};

/**
 * Who this person is connected to — and the form that records a new edge.
 *
 * The edge is stored once, directed, and always written from this card's
 * contact outward: the type selector's label completes the sentence
 * "<this contact> <type> <other contact>", and a live preview spells that
 * sentence out so the direction is never a guess. The other endpoint is picked
 * through the same suggestion engine the global search uses, so anyone
 * findable is linkable.
 */
export function RelationshipsCard({ contact }: { contact: ContactWithRelations }) {
  const createRelationship = useCreateRelationship();
  const deleteRelationship = useDeleteRelationship();

  const [open, setOpen] = useState(false);
  const [type, setType] = useState<RelationshipType>('knows');
  const [query, setQuery] = useState('');
  const [chosen, setChosen] = useState<{ id: Ulid; label: string } | null>(null);
  const [notes, setNotes] = useState('');

  const { data: suggestions = [] } = useSuggestions(chosen ? '' : query);
  const candidates = suggestions.filter(
    (suggestion) =>
      suggestion.kind === 'contact' && suggestion.id !== null && suggestion.id !== contact.id,
  );

  const reset = () => {
    setOpen(false);
    setType('knows');
    setQuery('');
    setChosen(null);
    setNotes('');
  };

  const save = async () => {
    if (!chosen) return;
    try {
      await createRelationship.mutateAsync({
        fromContactId: contact.id,
        toContactId: chosen.id,
        type,
        notes: notes.trim() || undefined,
      });
      toast.success('הקשר נשמר');
      reset();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : 'שמירת הקשר נכשלה');
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">קשרים</CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        {contact.relationships.map((edge) => {
          // An edge is stored once, directed. Rendering it from the far end
          // means reading it through the inverse type.
          const farType = edge.direction === 'out' ? edge.type : RELATIONSHIP_INVERSES[edge.type];
          const label =
            edge.direction === 'out'
              ? RELATIONSHIP_LABELS[edge.type].out
              : RELATIONSHIP_LABELS[farType].in;

          return (
            <div
              key={`${edge.id}-${edge.direction}`}
              className="flex items-center gap-2 text-sm"
              data-testid="relationship-row"
            >
              <span className="text-muted-foreground">{label}</span>
              <Link
                to={`/contacts/${edge.otherContact.id}`}
                className="font-medium text-primary hover:underline"
              >
                {edge.otherContact.displayName}
              </Link>
              {edge.notes ? (
                <span className="text-xs text-muted-foreground">— {edge.notes}</span>
              ) : null}
              <AlertDialog>
                <AlertDialogTrigger asChild>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="ms-auto"
                    aria-label={`מחיקת הקשר עם ${edge.otherContact.displayName}`}
                  >
                    <Trash2 className="size-3.5 text-destructive" />
                  </Button>
                </AlertDialogTrigger>
                <AlertDialogContent>
                  <AlertDialogHeader>
                    <AlertDialogTitle>
                      למחוק את הקשר עם {edge.otherContact.displayName}?
                    </AlertDialogTitle>
                    <AlertDialogDescription>
                      הקשר יוסר משני הכרטיסים. אנשי הקשר עצמם אינם נמחקים.
                    </AlertDialogDescription>
                  </AlertDialogHeader>
                  <AlertDialogFooter>
                    <AlertDialogCancel>ביטול</AlertDialogCancel>
                    <AlertDialogAction
                      data-testid="confirm-delete-relationship"
                      onClick={() =>
                        deleteRelationship.mutate(edge.id, {
                          onSuccess: () => toast.success('הקשר נמחק'),
                          onError: () => toast.error('מחיקת הקשר נכשלה'),
                        })
                      }
                    >
                      מחיקה
                    </AlertDialogAction>
                  </AlertDialogFooter>
                </AlertDialogContent>
              </AlertDialog>
            </div>
          );
        })}

        {contact.relationships.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            עוד לא נרשם כאן מי מכיר את מי. הקשרים הם הדרך למצוא אדם דרך מי שהכיר אותו.
          </p>
        ) : null}

        {open ? (
          <div className="space-y-3 rounded-md border p-3">
            <div className="flex flex-wrap items-center gap-2">
              <Select value={type} onValueChange={(value) => setType(value as RelationshipType)}>
                <SelectTrigger className="w-44" data-testid="relationship-type">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {RELATIONSHIP_TYPES.map((candidate) => (
                    <SelectItem key={candidate} value={candidate}>
                      {RELATIONSHIP_LABELS[candidate].out}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>

              {chosen ? (
                <div
                  className="flex items-center gap-1 rounded-md border py-1 ps-3 pe-1 text-sm"
                  data-testid="relationship-chosen"
                >
                  <span className="font-medium">{chosen.label}</span>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="size-6"
                    aria-label="ניקוי בחירה"
                    onClick={() => setChosen(null)}
                  >
                    <X className="size-3.5" />
                  </Button>
                </div>
              ) : (
                <Input
                  className="min-w-44 flex-1"
                  placeholder="שם איש הקשר השני…"
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  data-testid="relationship-contact"
                  aria-label="חיפוש איש הקשר השני"
                />
              )}
            </div>

            {!chosen && candidates.length > 0 ? (
              <div className="flex flex-col overflow-hidden rounded-md border">
                {candidates.map((candidate) => (
                  <button
                    key={candidate.id}
                    type="button"
                    className="px-3 py-1.5 text-start text-sm hover:bg-accent"
                    data-testid="relationship-suggestion"
                    onClick={() => {
                      setChosen({ id: candidate.id!, label: candidate.label });
                      setQuery('');
                    }}
                  >
                    {candidate.label}
                    {candidate.sublabel ? (
                      <span className="text-muted-foreground"> · {candidate.sublabel}</span>
                    ) : null}
                  </button>
                ))}
              </div>
            ) : null}

            <Input
              placeholder="הערה על הקשר (לא חובה)"
              value={notes}
              onChange={(event) => setNotes(event.target.value)}
              data-testid="relationship-notes"
              aria-label="הערה על הקשר"
            />

            {/* The sentence the saved edge will read as, so direction is explicit. */}
            <p className="text-xs text-muted-foreground">
              {contact.displayName} {RELATIONSHIP_LABELS[type].out} {chosen?.label ?? '…'}
            </p>

            <div className="flex gap-2">
              <Button
                onClick={() => void save()}
                disabled={!chosen || createRelationship.isPending}
                data-testid="save-relationship"
              >
                שמירת קשר
              </Button>
              <Button variant="ghost" onClick={reset}>
                ביטול
              </Button>
            </div>
          </div>
        ) : (
          <Button variant="outline" size="sm" onClick={() => setOpen(true)} data-testid="add-relationship">
            <Plus className="size-4" aria-hidden />
            הוספת קשר
          </Button>
        )}
      </CardContent>
    </Card>
  );
}
