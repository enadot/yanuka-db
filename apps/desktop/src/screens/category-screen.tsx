import { useState } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import { ArrowRight, Pencil, Search, UserMinus, UserPlus, Wand2 } from 'lucide-react';
import { describeRule, formatSubtitle, initials } from '@yanuka/core';
import type { CategoryMember } from '@yanuka/types';
import { countryName } from '@yanuka/utils';
import {
  Badge,
  Button,
  ContactAvatar,
  EmptyState,
  Input,
  Popover,
  PopoverContent,
  PopoverTrigger,
  Skeleton,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@yanuka/ui';
import { toast } from 'sonner';
import { CategoryEditorDialog } from '../components/categories/category-editor';
import { CategoryIcon } from '../components/categories/category-icon';
import { RELATIONSHIP_LABELS } from '../components/contact/relationships-card';
import { useDebouncedValue } from '../hooks/use-debounced-value';
import {
  useCategory,
  useCategoryMembers,
  useSetCategoryMembership,
  useSuggestions,
} from '../hooks/use-contacts';

const RULE_LABELS = {
  country: (code: string) => countryName(code) ?? code,
  relationship: (type: string) =>
    RELATIONSHIP_LABELS[type as keyof typeof RELATIONSHIP_LABELS]?.out ?? type,
};

/** One shelf: who is on it, why, and the two hand controls. */
export function CategoryScreen() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { data: category, isLoading } = useCategory(id);
  const [query, setQuery] = useState('');
  const debounced = useDebouncedValue(query, 200);
  const { data: members } = useCategoryMembers(id, debounced);
  const [editorOpen, setEditorOpen] = useState(false);
  const setMembership = useSetCategoryMembership();

  if (isLoading) {
    return (
      <div className="mx-auto max-w-4xl space-y-4 p-6">
        <Skeleton className="h-16 w-full" />
        <Skeleton className="h-64 w-full" />
      </div>
    );
  }

  if (!category) {
    return (
      <div className="mx-auto max-w-4xl p-6">
        <EmptyState
          title="הקטגוריה לא נמצאה"
          description="ייתכן שנמחקה."
          action={
            <Button asChild variant="outline">
              <Link to="/categories">לכל הקטגוריות</Link>
            </Button>
          }
        />
      </div>
    );
  }

  const change = (contactId: string, mode: 'include' | 'exclude' | 'auto') =>
    setMembership.mutate(
      { categoryId: category.id, contactId, mode },
      { onError: (error) => toast.error(error.message) },
    );

  return (
    <div className="mx-auto max-w-4xl space-y-6 p-6">
      <Button asChild variant="ghost" size="sm" className="-ms-2 gap-1">
        <Link to="/categories">
          <ArrowRight className="size-4" aria-hidden />
          כל הקטגוריות
        </Link>
      </Button>

      <header className="flex items-start gap-4">
        <CategoryIcon icon={category.icon} color={category.color} size="lg" />
        <div className="min-w-0 flex-1 space-y-1">
          <h1 className="flex items-center gap-2 text-2xl font-semibold">
            <span data-testid="category-title">{category.name}</span>
            <Badge variant="secondary" className="numeric font-normal" data-testid="category-count">
              {category.count}
            </Badge>
          </h1>
          {category.description ? (
            <p className="text-muted-foreground">{category.description}</p>
          ) : null}
          <p className="flex items-center gap-1 text-xs text-muted-foreground">
            {category.rule ? <Wand2 className="size-3" aria-hidden /> : null}
            {describeRule(category.rule, RULE_LABELS)}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <AddMember onPick={(contactId) => change(contactId, 'include')} />
          <Button variant="outline" size="sm" onClick={() => setEditorOpen(true)}>
            <Pencil className="size-4" aria-hidden />
            עריכה
          </Button>
        </div>
      </header>

      <div className="relative">
        <Search className="pointer-events-none absolute end-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
        <Input
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="חיפוש בתוך הקטגוריה"
          aria-label="חיפוש בתוך הקטגוריה"
          className="pe-9"
        />
      </div>

      {!members ? (
        <Skeleton className="h-64 w-full" />
      ) : members.items.length === 0 ? (
        <EmptyState
          title={query ? 'אין התאמות לחיפוש' : 'הקטגוריה ריקה'}
          description={
            category.rule
              ? 'אף איש קשר אינו מקיים את הכלל כרגע. אפשר לעדכן את הכלל, או לצרף מישהו ביד.'
              : 'צרפו אנשי קשר ביד מכאן, או מכרטיס איש הקשר.'
          }
        />
      ) : (
        <ul className="divide-y rounded-lg border bg-card" data-testid="category-members">
          {members.items.map((member) => (
            <MemberRow
              key={member.contact.id}
              member={member}
              onOpen={() => navigate(`/contacts/${member.contact.id}`)}
              onRemove={() =>
                change(member.contact.id, member.membership === 'rule' ? 'exclude' : 'auto')
              }
            />
          ))}
        </ul>
      )}
      {members && members.total > members.items.length ? (
        <p className="text-center text-xs text-muted-foreground">
          מוצגים {members.items.length} מתוך {members.total}. חפשו כדי לצמצם.
        </p>
      ) : null}

      <CategoryEditorDialog open={editorOpen} onOpenChange={setEditorOpen} category={category} />
    </div>
  );
}

function MemberRow({
  member,
  onOpen,
  onRemove,
}: {
  member: CategoryMember;
  onOpen: () => void;
  onRemove: () => void;
}) {
  const { contact, membership } = member;
  return (
    <li className="flex items-center gap-3 p-3" data-testid="category-member">
      <button
        type="button"
        onClick={onOpen}
        className="flex min-w-0 flex-1 items-center gap-3 text-start"
      >
        <ContactAvatar
          size="sm"
          name={contact.displayName}
          initials={initials(contact.displayName)}
        />
        <div className="min-w-0">
          <p className="truncate text-sm font-medium">
            {contact.prefix ? (
              <span className="text-muted-foreground">{contact.prefix} </span>
            ) : null}
            {contact.displayName}
          </p>
          <p className="truncate text-xs text-muted-foreground">{formatSubtitle(contact)}</p>
        </div>
      </button>
      <Badge variant="outline" className="gap-1 font-normal">
        {membership === 'rule' ? (
          <>
            <Wand2 className="size-3" aria-hidden />
            לפי הכלל
          </>
        ) : (
          'ידני'
        )}
      </Badge>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button variant="ghost" size="icon" aria-label="הסרה מהקטגוריה" onClick={onRemove}>
            <UserMinus className="size-4" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>
          {membership === 'rule' ? 'להוציא מהקטגוריה למרות הכלל' : 'לבטל את השיוך הידני'}
        </TooltipContent>
      </Tooltip>
    </li>
  );
}

/** Search-and-pick popover for pinning a contact into the shelf. */
function AddMember({ onPick }: { onPick: (contactId: string) => void }) {
  const [open, setOpen] = useState(false);
  const [text, setText] = useState('');
  const debounced = useDebouncedValue(text, 150);
  const { data: suggestions = [] } = useSuggestions(debounced);
  const contacts = suggestions.filter(
    (suggestion) => suggestion.kind === 'contact' && suggestion.id,
  );

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button variant="outline" size="sm" data-testid="category-add-member">
          <UserPlus className="size-4" aria-hidden />
          צירוף איש קשר
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-80 space-y-2 p-2">
        <Input
          autoFocus
          value={text}
          onChange={(event) => setText(event.target.value)}
          placeholder="שם, מקצוע או עיר"
          aria-label="חיפוש איש קשר לצירוף"
        />
        <ul className="max-h-64 overflow-y-auto">
          {contacts.map((suggestion) => (
            <li key={suggestion.id}>
              <button
                type="button"
                className="flex w-full flex-col items-start rounded-md p-2 text-start hover:bg-accent"
                onClick={() => {
                  onPick(suggestion.id!);
                  setOpen(false);
                  setText('');
                }}
              >
                <span className="text-sm">{suggestion.label}</span>
                {suggestion.sublabel ? (
                  <span className="text-xs text-muted-foreground">{suggestion.sublabel}</span>
                ) : null}
              </button>
            </li>
          ))}
          {debounced && contacts.length === 0 ? (
            <li className="p-2 text-sm text-muted-foreground">לא נמצאו התאמות</li>
          ) : null}
        </ul>
      </PopoverContent>
    </Popover>
  );
}
