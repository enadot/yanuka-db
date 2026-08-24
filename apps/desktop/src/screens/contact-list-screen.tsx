import { useState } from 'react';
import { Link } from 'react-router-dom';
import { Plus, Star, Users } from 'lucide-react';
import { formatSubtitle, initials } from '@yanuka/core';
import { formatRelative } from '@yanuka/utils';
import {
  Button,
  ContactAvatar,
  EmptyState,
  Pagination,
  PaginationContent,
  PaginationItem,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Skeleton,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
  TagPill,
  ToggleGroup,
  ToggleGroupItem,
} from '@yanuka/ui';
import { useContactList } from '../hooks/use-contacts';

/** Hebrew alphabet, plus Latin and a bucket for everything else. */
const ALPHABET = [...'אבגדהוזחטיכלמנסעפצקרשת'.split(''), 'A', 'M', 'S', '#'];

/**
 * Browse the whole database.
 *
 * Pagination is keyset-based — the cursor is the last row of the previous page
 * — so page 900 costs exactly what page 1 costs. That rules out numbered page
 * links, which would need OFFSET and would make SQLite walk every skipped row.
 * The alphabet index replaces them: it is both faster and closer to how someone
 * actually navigates a list of names. See docs/DECISIONS.md ADR-016.
 */
export function ContactListScreen() {
  const [cursor, setCursor] = useState<string | null>(null);
  const [history, setHistory] = useState<Array<string | null>>([]);
  const [letter, setLetter] = useState<string | null>(null);
  const [sort, setSort] = useState<'name' | 'recently_updated' | 'recently_added'>('name');
  const [limit, setLimit] = useState(50);

  const { data, isFetching } = useContactList({
    cursor,
    limit,
    sort,
    startsWith: letter,
    favoritesOnly: false,
    includeDeleted: false,
  });

  const reset = (mutate: () => void) => {
    // Any change to filtering or ordering invalidates the cursor: it points at
    // a position in a sequence that no longer exists.
    setCursor(null);
    setHistory([]);
    mutate();
  };

  return (
    <div className="mx-auto max-w-6xl space-y-4 p-6">
      <header className="flex items-center justify-between gap-3">
        <div>
          <h1 className="text-xl font-semibold">אנשי קשר</h1>
          {data ? (
            <p className="text-sm text-muted-foreground">
              <span className="numeric">{data.total}</span> רשומות במאגר
            </p>
          ) : null}
        </div>

        <div className="flex items-center gap-2">
          <Select
            value={sort}
            onValueChange={(value) => reset(() => setSort(value as typeof sort))}
          >
            <SelectTrigger className="w-44" aria-label="מיון">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="name">לפי שם</SelectItem>
              <SelectItem value="recently_updated">עודכנו לאחרונה</SelectItem>
              <SelectItem value="recently_added">נוספו לאחרונה</SelectItem>
            </SelectContent>
          </Select>

          <Button asChild>
            <Link to="/contacts/new">
              <Plus className="size-4" aria-hidden />
              איש קשר חדש
            </Link>
          </Button>
        </div>
      </header>

      <ToggleGroup
        type="single"
        value={letter ?? ''}
        onValueChange={(value) => reset(() => setLetter(value || null))}
        className="flex-wrap justify-start"
        aria-label="דילוג לאות"
      >
        {/* 44px targets: this is a row of 30 adjacent buttons, and at the old
            size hitting ג instead of ב was a coin toss. */}
        {ALPHABET.map((char) => (
          <ToggleGroupItem
            key={char}
            value={char}
            className="size-11 p-0 text-base font-semibold data-[state=on]:font-bold"
          >
            {char}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>

      {isFetching && !data ? (
        <div className="space-y-2">
          {Array.from({ length: 8 }, (_, index) => (
            <Skeleton key={index} className="h-12 w-full" />
          ))}
        </div>
      ) : data && data.items.length === 0 ? (
        <EmptyState
          icon={<Users />}
          title="אין רשומות להצגה"
          description={
            letter
              ? `לא נמצאו אנשי קשר שמתחילים באות ${letter}.`
              : 'המאגר ריק. אפשר להתחיל בהוספת איש קשר.'
          }
          action={
            <Button asChild variant="outline">
              <Link to="/contacts/new">הוספת איש קשר</Link>
            </Button>
          }
        />
      ) : (
        <div className="rounded-lg border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="w-[28rem]">שם</TableHead>
                <TableHead>מקצוע ומקום</TableHead>
                <TableHead className="w-56">תגיות</TableHead>
                <TableHead className="w-32">עודכן</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {data?.items.map((contact) => (
                <TableRow key={contact.id} className="row-virtual">
                  <TableCell>
                    <Link
                      to={`/contacts/${contact.id}`}
                      className="flex items-center gap-3 hover:underline"
                    >
                      <ContactAvatar
                        size="sm"
                        name={contact.displayName}
                        initials={initials(contact.displayName)}
                      />
                      <span className="font-medium">
                        {contact.prefix ? (
                          <span className="text-muted-foreground">{contact.prefix} </span>
                        ) : null}
                        {contact.displayName}
                      </span>
                      {contact.isFavorite ? (
                        <Star
                          className="size-3.5 fill-amber-400 text-amber-400"
                          aria-label="מועדף"
                        />
                      ) : null}
                    </Link>
                  </TableCell>
                  <TableCell className="text-muted-foreground">
                    {formatSubtitle(contact) || '—'}
                  </TableCell>
                  <TableCell>
                    <div className="flex flex-wrap gap-1">
                      {contact.tags.slice(0, 3).map((tag) => (
                        <TagPill key={tag} name={tag} />
                      ))}
                    </div>
                  </TableCell>
                  <TableCell className="text-xs text-muted-foreground">
                    {formatRelative(contact.updatedAt)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}

      <div className="flex items-center justify-between">
        <Select
          value={String(limit)}
          onValueChange={(value) => reset(() => setLimit(Number(value)))}
        >
          <SelectTrigger className="w-32" aria-label="שורות בעמוד">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {[25, 50, 100].map((size) => (
              <SelectItem key={size} value={String(size)}>
                {size} בעמוד
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        <Pagination className="mx-0 w-auto">
          <PaginationContent>
            <PaginationItem>
              <Button
                variant="ghost"
                size="sm"
                disabled={history.length === 0}
                onClick={() => {
                  const previous = [...history];
                  const target = previous.pop() ?? null;
                  setHistory(previous);
                  setCursor(target);
                }}
              >
                הקודם
              </Button>
            </PaginationItem>
            <PaginationItem>
              <Button
                variant="ghost"
                size="sm"
                disabled={!data?.nextCursor}
                onClick={() => {
                  setHistory((previous) => [...previous, cursor]);
                  setCursor(data?.nextCursor ?? null);
                }}
              >
                הבא
              </Button>
            </PaginationItem>
          </PaginationContent>
        </Pagination>
      </div>
    </div>
  );
}
