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
  const matchedFields = [
    ...new Set(reasons.map((reason) => SOURCE_LABELS[reason.source] ?? reason.source)),
  ];

  return (
    <Link
      to={`/contacts/${contact.id}`}
      className={cn(
        'row-virtual flex items-start gap-4 rounded-xl border-2 border-transparent p-4 transition-colors',
        'hover:border-border hover:bg-accent/50 focus-visible:border-ring focus-visible:outline-none',
      )}
    >
      <ContactAvatar
        size="lg"
        name={contact.displayName}
        initials={initials(contact.displayName)}
      />

      <div className="min-w-0 flex-1 space-y-1.5">
        {/*
          The name is the anchor the eye lands on, so it is the only thing in
          the row set heavy and large. Everything under it is deliberately
          quieter — a row where four things compete is a row that has to be
          read rather than scanned.
        */}
        <div className="flex items-baseline gap-2">
          <span className="truncate text-lg font-bold">
            {contact.prefix ? (
              <span className="font-medium text-muted-foreground">{contact.prefix} </span>
            ) : null}
            {contact.displayName}
          </span>
          {contact.isFavorite ? (
            <Star className="size-4 shrink-0 fill-amber-400 text-amber-400" aria-label="מועדף" />
          ) : null}
          {showScore ? (
            <span className="numeric ms-auto text-xs text-muted-foreground">{result.score}</span>
          ) : null}
        </div>

        {formatSubtitle(contact) ? (
          <p className="truncate text-base text-muted-foreground">{formatSubtitle(contact)}</p>
        ) : null}

        {contact.primaryPhone ? (
          <p className="numeric text-base font-medium text-muted-foreground">
            {contact.primaryPhone}
          </p>
        ) : null}

        {/*
          The sentence the query actually hit. For someone who has forgotten a
          name this is frequently the thing that confirms the right person, so
          it is given a visible quote treatment rather than another grey line.
        */}
        {snippet ? (
          <p className="line-clamp-2 rounded-lg border-s-4 border-s-primary/30 bg-muted/60 px-3 py-2 text-base">
            {snippet}
          </p>
        ) : null}

        <div className="flex flex-wrap items-center gap-2 pt-1">
          {contact.tags.slice(0, 4).map((tag) => (
            <TagPill key={tag} name={tag} />
          ))}
          {matchedFields.length > 0 ? (
            <Badge variant="outline" className="font-medium text-muted-foreground">
              נמצא לפי: {matchedFields.slice(0, 3).join(', ')}
            </Badge>
          ) : null}
        </div>
      </div>
    </Link>
  );
}
