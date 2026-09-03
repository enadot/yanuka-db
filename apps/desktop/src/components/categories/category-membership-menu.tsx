import { useState } from 'react';
import { Tags } from 'lucide-react';
import type { ContactWithRelations } from '@yanuka/types';
import { Button, Checkbox, Popover, PopoverContent, PopoverTrigger } from '@yanuka/ui';
import { toast } from 'sonner';
import { useCategories, useSetCategoryMembership } from '../../hooks/use-contacts';
import { CategoryIcon } from './category-icon';

/**
 * Tick a contact into or out of any category, from the card.
 *
 * Ticking on pins the person in (`include`); ticking off keeps them out even
 * if a rule would have chosen them (`exclude`). "Let the rule decide" is
 * offered on the category screen, where the distinction is visible.
 */
export function CategoryMembershipMenu({ contact }: { contact: ContactWithRelations }) {
  const [open, setOpen] = useState(false);
  const { data: categories = [] } = useCategories();
  const setMembership = useSetCategoryMembership();
  const current = new Map(contact.categories.map((category) => [category.id, category.membership]));

  const toggle = async (categoryId: string, checked: boolean) => {
    try {
      await setMembership.mutateAsync({
        categoryId,
        contactId: contact.id,
        mode: checked ? 'include' : 'exclude',
      });
    } catch (error) {
      toast.error(error instanceof Error ? error.message : 'השינוי נכשל');
    }
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button variant="outline" size="sm" data-testid="contact-categories-menu">
          <Tags className="size-4" aria-hidden />
          קטגוריות
        </Button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-72 p-2">
        {categories.length === 0 ? (
          <p className="p-2 text-sm text-muted-foreground">אין עדיין קטגוריות.</p>
        ) : (
          <ul className="max-h-80 space-y-0.5 overflow-y-auto">
            {categories.map((category) => {
              const membership = current.get(category.id);
              const checked = membership != null;
              return (
                <li key={category.id}>
                  <label className="flex cursor-pointer items-center gap-2 rounded-md p-1.5 hover:bg-accent">
                    <Checkbox
                      checked={checked}
                      onCheckedChange={(next) => void toggle(category.id, next === true)}
                      aria-label={category.name}
                      data-testid={`membership-${category.id}`}
                    />
                    <CategoryIcon icon={category.icon} color={category.color} size="sm" />
                    <span className="flex-1 truncate text-sm">{category.name}</span>
                    {membership === 'rule' ? (
                      <span className="text-xs text-muted-foreground">לפי הכלל</span>
                    ) : null}
                  </label>
                </li>
              );
            })}
          </ul>
        )}
      </PopoverContent>
    </Popover>
  );
}
