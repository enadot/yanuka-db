import { useState } from 'react';
import { Link } from 'react-router-dom';
import { Plus, Trash2 } from 'lucide-react';
import { toast } from 'sonner';
import {
  RELATIONSHIP_INVERSES,
  RELATIONSHIP_TYPES,
  type ContactWithRelations,
  type RelationshipType,
} from '@yanuka/types';
import {
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
import {
  useCreateRelationship,
  useDeleteRelationship,
  useSuggestions,
} from '../../hooks/use-contacts';
import { useDebouncedValue } from '../../hooks/use-debounced-value';

/** How each relationship reads from each of its two ends. */
const RELATIONSHIP_LABELS: Record<RelationshipType, { out: string; in: string }> = {
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
 * Every way an edge can be phrased from the card it is being added on.
 *
 * An edge is stored once and directed, but the person entering it is on one
 * particular card and remembers it from that side: standing on the recommended
 * contact, the natural sentence is "הומלץ על ידי הרב", not "הרב המליץ עליו".
 * Offering only the outgoing phrasing would mean the user has to work out which
 * of the two cards the relationship "belongs" to before they can write it down.
 * The reversed choices simply swap the endpoints on save.
 *
 * Symmetric types read identically from both ends, so they appear once.
 */
const RELATIONSHIP_CHOICES: { type: RelationshipType; reversed: boolean; label: string }[] =
  RELATIONSHIP_TYPES.flatMap((type) => {
    const { out, in: inward } = RELATIONSHIP_LABELS[type];
    const outward = { type, reversed: false, label: out };
    return out === inward ? [outward] : [outward, { type, reversed: true, label: inward }];
  });

export interface RelationshipEditorProps {
  contact: ContactWithRelations;
}

/**
 * The connections between people, readable and writable from the card.
 *
 * This is the part of the archive the product is actually named for: "the Jew
 * from London the rabbi recommended" is a question about an edge, not about a
 * field. Until an edge could be recorded here it could only arrive with the
 * demo data, which meant the one thing the user is most likely to remember was
 * the one thing they could not write down.
 *
 * The other end is chosen from search results rather than typed, because an
 * edge to a name-shaped string is not traversable and would quietly answer
 * "who else did he recommend" with nothing.
 */
export function RelationshipEditor({ contact }: RelationshipEditorProps) {
  const [open, setOpen] = useState(false);
  // The index into RELATIONSHIP_CHOICES, because the same type appears twice
  // with opposite endpoints and the type alone cannot say which was picked.
  const [choiceIndex, setChoiceIndex] = useState('0');
  const [query, setQuery] = useState('');
  const [notes, setNotes] = useState('');
  const debouncedQuery = useDebouncedValue(query, 250);
  const { data: suggestions = [] } = useSuggestions(debouncedQuery);

  const createRelationship = useCreateRelationship();
  const deleteRelationship = useDeleteRelationship();

  const linkedIds = new Set(contact.relationships.map((edge) => edge.otherContact.id));
  const candidates = suggestions.filter(
    (suggestion) =>
      suggestion.kind === 'contact' &&
      suggestion.id != null &&
      suggestion.id !== contact.id &&
      !linkedIds.has(suggestion.id),
  );

  const reset = () => {
    setOpen(false);
    setQuery('');
    setNotes('');
    setChoiceIndex('0');
  };

  const link = async (otherContactId: string) => {
    const choice = RELATIONSHIP_CHOICES[Number(choiceIndex)]!;
    try {
      await createRelationship.mutateAsync({
        fromContactId: choice.reversed ? otherContactId : contact.id,
        toContactId: choice.reversed ? contact.id : otherContactId,
        type: choice.type,
        notes: notes.trim() || null,
      });
      toast.success('הקשר נוסף');
      reset();
    } catch {
      toast.error('הוספת הקשר נכשלה');
    }
  };

  const unlink = async (id: string) => {
    try {
      await deleteRelationship.mutateAsync(id);
      toast.success('הקשר הוסר');
    } catch {
      toast.error('הסרת הקשר נכשלה');
    }
  };

  return (
    <Card>
      <CardHeader className="flex-row items-center justify-between">
        <CardTitle className="text-base">קשרים</CardTitle>
        <Button type="button" variant="outline" size="sm" onClick={() => setOpen((was) => !was)}>
          <Plus className="size-4" aria-hidden />
          הוספת קשר
        </Button>
      </CardHeader>
      <CardContent className="space-y-3">
        {contact.relationships.length === 0 && !open ? (
          <p className="text-sm text-muted-foreground">
            לא נרשמו קשרים. מי הכיר, מי המליץ, ומי עוד מאותו מקום — זה לרוב מה שזוכרים כששוכחים את
            השם.
          </p>
        ) : null}

        {contact.relationships.map((edge) => {
          // An edge is stored once, directed. Reading it from the far end means
          // presenting it through the inverse type.
          const edgeType = edge.direction === 'out' ? edge.type : RELATIONSHIP_INVERSES[edge.type];
          const label =
            edge.direction === 'out'
              ? RELATIONSHIP_LABELS[edge.type].out
              : RELATIONSHIP_LABELS[edgeType].in;

          return (
            <div key={`${edge.id}-${edge.direction}`} className="flex items-center gap-2 text-sm">
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
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="ms-auto"
                aria-label={`הסרת הקשר עם ${edge.otherContact.displayName}`}
                onClick={() => void unlink(edge.id)}
              >
                <Trash2 className="size-4 text-destructive" />
              </Button>
            </div>
          );
        })}

        {open ? (
          <div className="space-y-2 rounded-md border p-3">
            <div className="flex items-center gap-2">
              <Select value={choiceIndex} onValueChange={setChoiceIndex}>
                <SelectTrigger className="w-52" aria-label="סוג הקשר">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {RELATIONSHIP_CHOICES.map((choice, index) => (
                    <SelectItem key={`${choice.type}-${choice.reversed}`} value={String(index)}>
                      {choice.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>

              <Input
                className="flex-1"
                placeholder="חיפוש איש קשר"
                aria-label="חיפוש איש קשר לקישור"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
              />
            </div>

            <Input
              placeholder="הערה על הקשר — למשל, מאיפה הם מכירים"
              aria-label="הערה על הקשר"
              value={notes}
              onChange={(event) => setNotes(event.target.value)}
            />

            {query.trim() ? (
              <div className="flex flex-wrap gap-2">
                {candidates.length === 0 ? (
                  <p className="text-sm text-muted-foreground">אין תוצאות מתאימות.</p>
                ) : null}
                {candidates.map((suggestion) => (
                  <Button
                    key={suggestion.id}
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={createRelationship.isPending}
                    onClick={() => void link(suggestion.id!)}
                  >
                    {suggestion.label}
                    {suggestion.sublabel ? (
                      <span className="text-xs text-muted-foreground">{suggestion.sublabel}</span>
                    ) : null}
                  </Button>
                ))}
              </div>
            ) : null}

            <Button type="button" variant="ghost" size="sm" onClick={reset}>
              ביטול
            </Button>
          </div>
        ) : null}
      </CardContent>
    </Card>
  );
}
