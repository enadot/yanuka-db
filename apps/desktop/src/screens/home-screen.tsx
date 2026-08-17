import { useState } from 'react';
import { Link, useSearchParams } from 'react-router-dom';
import { SearchX, Star, Clock, Plus } from 'lucide-react';
import type { FacetFilters } from '@yanuka/types';
import { formatSubtitle, initials } from '@yanuka/core';
import {
  Button,
  Card,
  CardContent,
  ContactAvatar,
  EmptyState,
  Input,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Skeleton,
} from '@yanuka/ui';
import { useDebouncedValue } from '../hooks/use-debounced-value';
import { useFavoriteContacts, useRecentContacts, useSearch } from '../hooks/use-contacts';
import { SearchResultRow } from '../components/search/search-result-row';
import { FacetPanel } from '../components/search/facet-panel';

/**
 * The home screen is the search box.
 *
 * The brief is explicit that this must not be a dashboard, and the reasoning
 * holds up: the job this application exists to do is "find the person I can
 * half remember". Anything else on this screen competes with that. Before a
 * query is typed the space is given to favourites and recently opened records,
 * which is the only content that is useful without a query.
 */
export function HomeScreen() {
  const [params, setParams] = useSearchParams();
  const [text, setText] = useState(params.get('q') ?? '');
  const [filters, setFilters] = useState<FacetFilters>({});
  const [sort, setSort] = useState<'relevance' | 'name' | 'recently_updated'>('relevance');

  const debounced = useDebouncedValue(text);
  const hasQuery = debounced.trim().length > 0 || Object.keys(filters).length > 0;

  const { data, isFetching } = useSearch(
    {
      text: debounced,
      filters,
      sort,
      limit: 50,
      offset: 0,
      favoritesOnly: false,
      includeDeleted: false,
    },
    hasQuery,
  );

  const updateText = (value: string) => {
    setText(value);
    // Reflected in the URL so a search can be linked to and survives reload.
    if (value) setParams({ q: value }, { replace: true });
    else setParams({}, { replace: true });
  };

  return (
    <div className="mx-auto flex max-w-5xl flex-col gap-6 p-6">
      <section className="space-y-3 pt-6 text-center">
        <h1 className="text-2xl font-semibold">את מי מחפשים?</h1>
        <Input
          autoFocus
          value={text}
          onChange={(event) => updateText(event.target.value)}
          placeholder="שם, מקום, מקצוע, מוסד, הערה או כל פרט שאתם זוכרים"
          className="h-12 text-base"
          aria-label="חיפוש אנשי קשר"
        />
        <p className="text-sm text-muted-foreground">
          אפשר לחפש גם לפי משפט מתוך הערה, מי המליץ, או סיבה שבגללה שמרתם את האיש
        </p>
      </section>

      {hasQuery ? (
        <SearchResults
          loading={isFetching}
          data={data}
          filters={filters}
          onFiltersChange={setFilters}
          sort={sort}
          onSortChange={setSort}
          query={debounced}
        />
      ) : (
        <StartingPoints />
      )}
    </div>
  );
}

function SearchResults({
  loading,
  data,
  filters,
  onFiltersChange,
  sort,
  onSortChange,
  query,
}: {
  loading: boolean;
  data: ReturnType<typeof useSearch>['data'];
  filters: FacetFilters;
  onFiltersChange: (filters: FacetFilters) => void;
  sort: 'relevance' | 'name' | 'recently_updated';
  onSortChange: (sort: 'relevance' | 'name' | 'recently_updated') => void;
  query: string;
}) {
  if (loading && !data) {
    return (
      <div className="space-y-2">
        {Array.from({ length: 5 }, (_, index) => (
          <Skeleton key={index} className="h-20 w-full" />
        ))}
      </div>
    );
  }

  if (!data) return null;

  if (data.results.length === 0) {
    return (
      <EmptyState
        icon={<SearchX />}
        title={`לא נמצאו תוצאות עבור "${query}"`}
        description="אפשר לנסות פרט אחר — עיר, מקצוע, מוסד, או מילה מתוך הערה שנכתבה על האדם."
        action={
          <Button asChild variant="outline">
            <Link to="/contacts/new">
              <Plus className="size-4" aria-hidden />
              הוספת איש קשר חדש
            </Link>
          </Button>
        }
      />
    );
  }

  return (
    <div className="flex gap-6">
      <FacetPanel facets={data.facets} filters={filters} onChange={onFiltersChange} />

      <div className="min-w-0 flex-1 space-y-3">
        <div className="flex items-center justify-between gap-3">
          <p className="text-sm text-muted-foreground">
            <span className="numeric font-medium text-foreground">{data.total}</span> תוצאות
            {data.tookMs > 0 ? (
              <span className="ms-2 text-xs">({data.tookMs} מילישניות)</span>
            ) : null}
          </p>

          <Select value={sort} onValueChange={(value) => onSortChange(value as typeof sort)}>
            <SelectTrigger className="w-40" aria-label="מיון תוצאות">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="relevance">לפי התאמה</SelectItem>
              <SelectItem value="name">לפי שם</SelectItem>
              <SelectItem value="recently_updated">לפי עדכון אחרון</SelectItem>
            </SelectContent>
          </Select>
        </div>

        <div className="space-y-1">
          {data.results.map((result) => (
            <SearchResultRow key={result.contact.id} result={result} />
          ))}
        </div>
      </div>
    </div>
  );
}

/** Favourites and recently opened, shown before anything is typed. */
function StartingPoints() {
  const { data: favorites = [] } = useFavoriteContacts();
  const { data: recent = [] } = useRecentContacts();

  if (favorites.length === 0 && recent.length === 0) return null;

  return (
    <div className="grid gap-6 md:grid-cols-2">
      {favorites.length > 0 ? (
        <ContactStrip title="מועדפים" icon={<Star className="size-4" />} contacts={favorites} />
      ) : null}
      {recent.length > 0 ? (
        <ContactStrip title="נצפו לאחרונה" icon={<Clock className="size-4" />} contacts={recent} />
      ) : null}
    </div>
  );
}

function ContactStrip({
  title,
  icon,
  contacts,
}: {
  title: string;
  icon: React.ReactNode;
  contacts: Array<Parameters<typeof formatSubtitle>[0] & { id: string; displayName: string }>;
}) {
  return (
    <section className="space-y-2">
      <h2 className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
        {icon}
        {title}
      </h2>
      <Card>
        <CardContent className="p-2">
          {contacts.map((contact) => (
            <Link
              key={contact.id}
              to={`/contacts/${contact.id}`}
              className="flex items-center gap-3 rounded-md p-2 transition-colors hover:bg-accent"
            >
              <ContactAvatar
                size="sm"
                name={contact.displayName}
                initials={initials(contact.displayName)}
              />
              <div className="min-w-0">
                <p className="truncate text-sm font-medium">{contact.displayName}</p>
                <p className="truncate text-xs text-muted-foreground">
                  {formatSubtitle(contact)}
                </p>
              </div>
            </Link>
          ))}
        </CardContent>
      </Card>
    </section>
  );
}
