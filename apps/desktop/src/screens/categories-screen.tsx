import { useState } from 'react';
import { Link } from 'react-router-dom';
import { ArrowDown, ArrowUp, Lightbulb, Pencil, Plus, Trash2, Wand2 } from 'lucide-react';
import { describeRule, type CategoryInput } from '@yanuka/core';
import type { CategorySuggestion, CategorySummary } from '@yanuka/types';
import { countryName } from '@yanuka/utils';
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
  Badge,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  EmptyState,
  Skeleton,
  Switch,
} from '@yanuka/ui';
import { toast } from 'sonner';
import { CategoryEditorDialog } from '../components/categories/category-editor';
import { CategoryIcon } from '../components/categories/category-icon';
import { RELATIONSHIP_LABELS } from '../components/contact/relationships-card';
import {
  useCategories,
  useCategorySuggestions,
  useDeleteCategory,
  useReorderCategories,
  useUpdateCategory,
} from '../hooks/use-contacts';

const RULE_LABELS = {
  country: (code: string) => countryName(code) ?? code,
  relationship: (type: string) =>
    RELATIONSHIP_LABELS[type as keyof typeof RELATIONSHIP_LABELS]?.out ?? type,
};

/**
 * The categories dashboard: every shelf, its size, its rule in a sentence,
 * and the controls to create, edit, order and remove them. Below, what the
 * archive itself suggests.
 */
export function CategoriesScreen() {
  const { data: categories, isLoading } = useCategories();
  const [editing, setEditing] = useState<CategorySummary | null>(null);
  const [initial, setInitial] = useState<Partial<CategoryInput> | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);

  const openNew = (seed: Partial<CategoryInput> | null = null) => {
    setEditing(null);
    setInitial(seed);
    setEditorOpen(true);
  };
  const openEdit = (category: CategorySummary) => {
    setEditing(category);
    setInitial(null);
    setEditorOpen(true);
  };

  return (
    <div className="mx-auto max-w-4xl space-y-6 p-6">
      <header className="flex items-start justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold">קטגוריות</h1>
          <p className="text-sm text-muted-foreground">
            המדפים של האוצר. קטגוריה חכמה מתמלאת מעצמה לפי כלל; כל אחת אפשר גם למלא ביד.
          </p>
        </div>
        <Button onClick={() => openNew()} data-testid="category-new">
          <Plus className="size-4" aria-hidden />
          קטגוריה חדשה
        </Button>
      </header>

      {isLoading ? (
        <div className="space-y-2">
          {Array.from({ length: 4 }, (_, index) => (
            <Skeleton key={index} className="h-20 w-full" />
          ))}
        </div>
      ) : !categories || categories.length === 0 ? (
        <EmptyState
          icon={<Wand2 />}
          title="אין עדיין קטגוריות"
          description="קטגוריה ראשונה לוקחת דקה: שם, אייקון, וכלל שאומר מי שייך."
          action={<Button onClick={() => openNew()}>קטגוריה חדשה</Button>}
        />
      ) : (
        <ul className="space-y-2" data-testid="category-list">
          {categories.map((category, index) => (
            <CategoryRow
              key={category.id}
              category={category}
              order={categories.map((c) => c.id)}
              index={index}
              onEdit={() => openEdit(category)}
            />
          ))}
        </ul>
      )}

      <Suggestions onPick={(suggestion) => openNew(suggestion)} />

      <CategoryEditorDialog
        open={editorOpen}
        onOpenChange={setEditorOpen}
        category={editing}
        initial={initial}
      />
    </div>
  );
}

function CategoryRow({
  category,
  order,
  index,
  onEdit,
}: {
  category: CategorySummary;
  order: string[];
  index: number;
  onEdit: () => void;
}) {
  const reorder = useReorderCategories();
  const update = useUpdateCategory();
  const remove = useDeleteCategory();

  const move = (direction: -1 | 1) => {
    const next = [...order];
    const target = index + direction;
    if (target < 0 || target >= next.length) return;
    [next[index], next[target]] = [next[target]!, next[index]!];
    reorder.mutate(next);
  };

  const toggleHome = (showOnHome: boolean) =>
    update.mutate({
      id: category.id,
      input: {
        name: category.name,
        description: category.description,
        parentId: category.parentId,
        icon: category.icon,
        color: category.color,
        rule: category.rule,
        showOnHome,
      },
    });

  return (
    <li
      className="flex items-center gap-3 rounded-lg border bg-card p-3"
      data-testid="category-row"
    >
      <div className="flex flex-col">
        <Button
          variant="ghost"
          size="icon"
          className="size-6"
          aria-label="הזזה למעלה"
          disabled={index === 0}
          onClick={() => move(-1)}
        >
          <ArrowUp className="size-3.5" />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          className="size-6"
          aria-label="הזזה למטה"
          disabled={index === order.length - 1}
          onClick={() => move(1)}
        >
          <ArrowDown className="size-3.5" />
        </Button>
      </div>

      <CategoryIcon icon={category.icon} color={category.color} />

      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <Link to={`/categories/${category.id}`} className="truncate font-medium hover:underline">
            {category.name}
          </Link>
          <Badge variant="secondary" className="numeric font-normal">
            {category.count}
          </Badge>
          {category.rule ? (
            <Badge variant="outline" className="gap-1 font-normal">
              <Wand2 className="size-3" aria-hidden />
              חכמה
            </Badge>
          ) : null}
        </div>
        {category.description ? (
          <p className="truncate text-sm text-muted-foreground">{category.description}</p>
        ) : null}
        <p
          className="truncate text-xs text-muted-foreground"
          title={describeRule(category.rule, RULE_LABELS)}
        >
          {describeRule(category.rule, RULE_LABELS)}
        </p>
      </div>

      <div className="flex items-center gap-1">
        <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
          במסך הבית
          <Switch
            checked={category.showOnHome}
            onCheckedChange={toggleHome}
            aria-label={`להציג את ${category.name} במסך הבית`}
          />
        </label>
        <Button variant="ghost" size="icon" aria-label="עריכה" onClick={onEdit}>
          <Pencil className="size-4" />
        </Button>
        <AlertDialog>
          <AlertDialogTrigger asChild>
            <Button variant="ghost" size="icon" aria-label="מחיקה">
              <Trash2 className="size-4 text-destructive" />
            </Button>
          </AlertDialogTrigger>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>למחוק את הקטגוריה "{category.name}"?</AlertDialogTitle>
              <AlertDialogDescription>
                אנשי הקשר עצמם נשארים כפי שהם; רק המדף נעלם, יחד עם השיוכים הידניים אליו.
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>ביטול</AlertDialogCancel>
              <AlertDialogAction
                onClick={() =>
                  remove.mutate(category.id, {
                    onSuccess: () => toast.success('הקטגוריה נמחקה'),
                    onError: (error) => toast.error(error.message),
                  })
                }
              >
                מחיקה
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </div>
    </li>
  );
}

function Suggestions({ onPick }: { onPick: (suggestion: Partial<CategoryInput>) => void }) {
  const { data: suggestions = [] } = useCategorySuggestions();
  if (suggestions.length === 0) return null;

  const pick = (suggestion: CategorySuggestion) =>
    onPick({
      name: suggestion.name,
      description: suggestion.description,
      icon: suggestion.icon,
      rule: suggestion.rule,
      showOnHome: true,
    });

  return (
    <Card data-testid="category-suggestions">
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <Lightbulb className="size-4" aria-hidden />
          הצעות מהמאגר
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-1">
        <p className="text-sm text-muted-foreground">
          מקצועות, תגיות ומקומות שחוזרים במאגר ועדיין אין להם מדף.
        </p>
        <ul className="divide-y">
          {suggestions.map((suggestion) => (
            <li
              key={`${suggestion.rule.conditions[0]?.field}-${suggestion.name}`}
              className="flex items-center gap-3 py-2"
            >
              <CategoryIcon icon={suggestion.icon} color={null} size="sm" />
              <span className="flex-1 text-sm">{suggestion.name}</span>
              <span className="numeric text-xs text-muted-foreground">{suggestion.count}</span>
              <Button variant="outline" size="sm" onClick={() => pick(suggestion)}>
                יצירה
              </Button>
            </li>
          ))}
        </ul>
      </CardContent>
    </Card>
  );
}
