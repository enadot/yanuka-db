import { Link } from 'react-router-dom';
import { Star } from 'lucide-react';
import type { SearchResult } from '@yanuka/types';
import { formatSubtitle, initials } from '@yanuka/core';
import { Badge, ContactAvatar, TagPill, cn } from '@yanuka/ui';

/** Hebrew label for each thing a match can have come from. */
const SOURCE_LABELS: Record<string, string> = {
  name: 'שם',
  alias: 'שם חלופי',
  phone: 'טלפון',
  email: 'אימייל',
  profession: 'מקצוע',
  role: 'תפקיד',
  specialty: 'התמחות',
  organization: 'מוסד',
  city: 'מקום',
  country: 'מדינה',
  tag: 'תגית',
  category: 'קטגוריה',
  notes: 'הערה',
  reason_for_saving: 'סיבת שמירה',
};

export interface SearchResultRowProps {
  result: SearchResult;
  /** Show the raw relevance score. Development aid, off by default. */
  showScore?: boolean;
}

/**
 * One search result.
 *
 * The row answers two questions at once: who is this, and why am I looking at
 * them. The second matters more than usual here — a user who has forgotten a
 * name is searching on a half-remembered fragment, and seeing the sentence
 * their query actually hit is often what confirms the right person.
 */
export function SearchResultRow({ result, showScore = false }: SearchResultRowProps) {
  const { contact, reasons } = result;

  const snippet = reasons.find((reason) => reason.snippet)?.snippet ?? null;
  const matchedFields = [...new Set(reasons.map((reason) => SOURCE_LABELS[reason.source] ?? reason.source))];

  return (
    <Link
      to={`/contacts/${contact.id}`}
      className={cn(
        'row-virtual flex gap-3 rounded-lg border border-transparent p-3 transition-colors',
        'hover:border-border hover:bg-accent/50 focus-visible:border-ring focus-visible:outline-none',
      )}
    >
      <ContactAvatar name={contact.displayName} initials={initials(contact.displayName)} />

      <div className="min-w-0 flex-1 space-y-1">
        <div className="flex items-baseline gap-2">
          <span className="truncate font-medium">
            {contact.prefix ? <span className="text-muted-foreground">{contact.prefix} </span> : null}
            {contact.displayName}
          </span>
          {contact.isFavorite ? (
            <Star className="size-3.5 shrink-0 fill-amber-400 text-amber-400" aria-label="מועדף" />
          ) : null}
          {showScore ? (
            <span className="numeric ms-auto text-xs text-muted-foreground">{result.score}</span>
          ) : null}
        </div>

        {formatSubtitle(contact) ? (
          <p className="truncate text-sm text-muted-foreground">{formatSubtitle(contact)}</p>
        ) : null}

        {contact.primaryPhone ? (
          <p className="numeric text-sm text-muted-foreground">{contact.primaryPhone}</p>
        ) : null}

        {snippet ? (
          <p className="line-clamp-2 rounded-md bg-muted/60 px-2 py-1 text-sm text-muted-foreground">
            {snippet}
          </p>
        ) : null}

        <div className="flex flex-wrap items-center gap-1.5 pt-0.5">
          {contact.tags.slice(0, 4).map((tag) => (
            <TagPill key={tag} name={tag} />
          ))}
          {matchedFields.length > 0 ? (
            <Badge variant="outline" className="font-normal text-muted-foreground">
              נמצא לפי: {matchedFields.slice(0, 3).join(', ')}
            </Badge>
          ) : null}
        </div>
      </div>
    </Link>
  );
}
