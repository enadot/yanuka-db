import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { Building2, Tag as TagIcon, User } from 'lucide-react';
import type { SearchSuggestion } from '@yanuka/types';
import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from '@yanuka/ui';
import { useDebouncedValue } from '../../hooks/use-debounced-value';
import { useSuggestions } from '../../hooks/use-contacts';

const GROUP_LABELS = {
  contact: 'אנשים',
  tag: 'תגיות',
  organization: 'מוסדות',
  category: 'קטגוריות',
  profession: 'מקצועות',
  city: 'ערים',
} as const;

const GROUP_ICONS = {
  contact: User,
  tag: TagIcon,
  organization: Building2,
  category: TagIcon,
  profession: TagIcon,
  city: TagIcon,
} as const;

export interface GlobalSearchDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/**
 * Ctrl+K search palette.
 *
 * The whole keyboard workflow the brief asks for lives here: Ctrl+K, type,
 * Enter, and the contact is open — no mouse at any point.
 */
export function GlobalSearchDialog({ open, onOpenChange }: GlobalSearchDialogProps) {
  const [text, setText] = useState('');
  const debounced = useDebouncedValue(text);
  const { data: suggestions = [], isFetching } = useSuggestions(debounced);
  const navigate = useNavigate();

  const grouped = suggestions.reduce<Record<string, SearchSuggestion[]>>(
    (acc: Record<string, SearchSuggestion[]>, suggestion: SearchSuggestion) => {
      (acc[suggestion.kind] ??= []).push(suggestion);
      return acc;
    },
    {},
  );

  const select = (kind: string, id: string | null, label: string) => {
    onOpenChange(false);
    setText('');
    if (kind === 'contact' && id) navigate(`/contacts/${id}`);
    // Selecting a tag or an organization is a request to browse, not to open a
    // record: it hands the label back to the full search screen as a query.
    else navigate(`/?q=${encodeURIComponent(label)}`);
  };

  return (
    <CommandDialog
      open={open}
      onOpenChange={onOpenChange}
      title="חיפוש מהיר"
      description="חיפוש לפי שם, מקום, מקצוע, מוסד, הערה או כל פרט שאתם זוכרים"
      // cmdk's built-in fuzzy filter must be off: ranking comes from the search
      // engine, which knows about Hebrew, aliases and phone formats. Leaving it
      // on would re-sort the results by a naive substring score.
      commandProps={{ shouldFilter: false }}
    >
      <CommandInput placeholder="את מי מחפשים?" value={text} onValueChange={setText} />
      <CommandList>
        {!isFetching && debounced.trim() && suggestions.length === 0 ? (
          <CommandEmpty>לא נמצאו תוצאות עבור "{debounced}"</CommandEmpty>
        ) : null}

        {Object.entries(grouped).map(([kind, items]) => {
          const Icon = GROUP_ICONS[kind as keyof typeof GROUP_ICONS] ?? User;
          return (
            <CommandGroup
              key={kind}
              heading={GROUP_LABELS[kind as keyof typeof GROUP_LABELS] ?? kind}
            >
              {items.map((suggestion) => (
                <CommandItem
                  key={`${kind}-${suggestion.id ?? suggestion.label}`}
                  value={`${kind}-${suggestion.id ?? suggestion.label}`}
                  onSelect={() => select(kind, suggestion.id, suggestion.label)}
                  className="gap-2"
                >
                  <Icon className="size-4 text-muted-foreground" aria-hidden />
                  <span className="flex-1">{suggestion.label}</span>
                  {suggestion.sublabel ? (
                    <span className="text-xs text-muted-foreground">{suggestion.sublabel}</span>
                  ) : null}
                  {suggestion.count != null ? (
                    <span className="numeric text-xs text-muted-foreground">
                      {suggestion.count}
                    </span>
                  ) : null}
                </CommandItem>
              ))}
            </CommandGroup>
          );
        })}
      </CommandList>
    </CommandDialog>
  );
}
