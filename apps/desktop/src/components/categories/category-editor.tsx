import { useMemo, useState } from 'react';
import { Plus, Trash2 } from 'lucide-react';
import { RULE_FIELD_LABELS, RULE_OPERATOR_LABELS, type CategoryInput } from '@yanuka/core';
import {
  CATEGORY_FIELD_OPERATORS,
  CATEGORY_RULE_FIELDS,
  RELATIONSHIP_TYPES,
  VALUELESS_OPERATORS,
  type CategoryRule,
  type CategoryRuleField,
  type CategoryRuleOperator,
  type CategorySummary,
} from '@yanuka/types';
import { COUNTRY_NAMES_HE } from '@yanuka/utils';
import { CategoryInputSchema } from '@yanuka/validation';
import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  Label,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Switch,
  Textarea,
  cn,
} from '@yanuka/ui';
import { toast } from 'sonner';
import { useDebouncedValue } from '../../hooks/use-debounced-value';
import { useCategoryPreview, useCreateCategory, useUpdateCategory } from '../../hooks/use-contacts';
import { RELATIONSHIP_LABELS } from '../contact/relationships-card';
import { CATEGORY_COLORS, CATEGORY_ICON_KEYS, CATEGORY_ICONS, CategoryIcon } from './category-icon';

/** One editable row of the rule builder; values are kept as typed. */
interface Row {
  key: number;
  field: CategoryRuleField;
  op: CategoryRuleOperator;
  /** Comma-separated alternatives for text fields; a single value otherwise. */
  text: string;
}

let rowKey = 0;
const newRow = (field: CategoryRuleField = 'occupation'): Row => ({
  key: (rowKey += 1),
  field,
  op: CATEGORY_FIELD_OPERATORS[field][0]!,
  text: '',
});

const FIELD_HINTS: Partial<Record<CategoryRuleField, string>> = {
  occupation: 'למשל: רב, דיין, ראש ישיבה',
  city: 'למשל: ירושלים, בני ברק',
  tag: 'שם תגית מדויק',
  notes: 'מילה או ביטוי מתוך ההערות',
  anywhere: 'מילה בכל שדה שהוא',
  organization: 'שם המוסד או חלק ממנו',
  specialty: 'תחום התמחות',
  name: 'שם או חלק ממנו',
  meaning: 'משפט חופשי, למשל: מי שיכול לעזור בבניית בית כנסת',
};

function rowsToRule(rows: Row[], match: 'all' | 'any'): CategoryRule | null {
  const conditions = rows.flatMap((row) => {
    const valueless = VALUELESS_OPERATORS.includes(row.op);
    const single =
      row.op === 'within_days' ||
      row.op === 'similar' ||
      row.field === 'country' ||
      row.field === 'relationship';
    const values = valueless
      ? []
      : single
        ? [row.text.trim()].filter(Boolean)
        : row.text
            .split(/[,،\n]/)
            .map((value) => value.trim())
            .filter(Boolean);
    if (!valueless && values.length === 0) return [];
    return [{ field: row.field, op: row.op, values }];
  });
  return conditions.length > 0 ? { match, conditions } : null;
}

function ruleToRows(rule: CategoryRule | null): Row[] {
  if (!rule) return [];
  return rule.conditions.map((condition) => ({
    key: (rowKey += 1),
    field: condition.field,
    op: condition.op,
    text: condition.values.join(', '),
  }));
}

export interface CategoryEditorDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Present when editing; absent when creating. */
  category?: CategorySummary | null;
  /** Starting values for a new category (e.g. from a suggestion). */
  initial?: Partial<CategoryInput> | null;
  onSaved?: (id: string) => void;
}

/**
 * Create or edit a category: its face, whether it sits on the home screen,
 * and the rule that fills it — with a live count of who the rule would pick.
 */
export function CategoryEditorDialog({
  open,
  onOpenChange,
  category,
  initial,
  onSaved,
}: CategoryEditorDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="max-h-[90vh] overflow-y-auto sm:max-w-2xl"
        data-testid="category-editor"
      >
        <DialogHeader>
          <DialogTitle>{category ? 'עריכת קטגוריה' : 'קטגוריה חדשה'}</DialogTitle>
          <DialogDescription>
            קטגוריה היא מדף באוצר. אפשר למלא אותה ביד, או להגדיר כלל — ואנשי קשר יצטרפו ויעזבו מעצמם
            כשהפרטים שלהם משתנים.
          </DialogDescription>
        </DialogHeader>
        {/* Mounted only while open, so every opening starts from its source. */}
        <EditorBody
          category={category ?? null}
          initial={initial ?? null}
          onClose={() => onOpenChange(false)}
          onSaved={onSaved}
        />
      </DialogContent>
    </Dialog>
  );
}

function EditorBody({
  category,
  initial,
  onClose,
  onSaved,
}: {
  category: CategorySummary | null;
  initial: Partial<CategoryInput> | null;
  onClose: () => void;
  onSaved?: (id: string) => void;
}) {
  const source: Partial<CategoryInput> = category
    ? {
        name: category.name,
        description: category.description,
        icon: category.icon,
        color: category.color,
        rule: category.rule,
        showOnHome: category.showOnHome,
      }
    : (initial ?? {});
  const [name, setName] = useState(source.name ?? '');
  const [description, setDescription] = useState(source.description ?? '');
  const [icon, setIcon] = useState<string | null>(source.icon ?? null);
  const [color, setColor] = useState<string | null>(source.color ?? CATEGORY_COLORS[0]!);
  const [showOnHome, setShowOnHome] = useState(source.showOnHome ?? true);
  const [smart, setSmart] = useState(source.rule != null);
  const [match, setMatch] = useState<'all' | 'any'>(source.rule?.match ?? 'all');
  const [rows, setRows] = useState<Row[]>(() =>
    source.rule ? ruleToRows(source.rule) : [newRow()],
  );

  const create = useCreateCategory();
  const update = useUpdateCategory();

  const rule = useMemo(() => (smart ? rowsToRule(rows, match) : null), [smart, rows, match]);
  const debouncedRule = useDebouncedValue(rule, 300);
  const preview = useCategoryPreview(debouncedRule);

  const updateRow = (key: number, patch: Partial<Row>) =>
    setRows((current) =>
      current.map((row) => {
        if (row.key !== key) return row;
        const next = { ...row, ...patch };
        // A field change resets the operator when the old one no longer fits.
        if (patch.field && !CATEGORY_FIELD_OPERATORS[next.field].includes(next.op)) {
          next.op = CATEGORY_FIELD_OPERATORS[next.field][0]!;
          next.text = '';
        }
        return next;
      }),
    );

  const save = async () => {
    const parsed = CategoryInputSchema.safeParse({
      name,
      description,
      parentId: null,
      icon,
      color,
      rule,
      showOnHome,
    });
    if (!parsed.success) {
      toast.error(parsed.error.issues[0]?.message ?? 'הקלט אינו תקין');
      return;
    }
    if (smart && !rule) {
      toast.error('כלל צריך לפחות תנאי אחד עם ערך');
      return;
    }
    try {
      const saved = category
        ? await update.mutateAsync({ id: category.id, input: parsed.data })
        : await create.mutateAsync(parsed.data);
      toast.success(category ? 'הקטגוריה עודכנה' : 'הקטגוריה נוצרה');
      onClose();
      onSaved?.(saved.id);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : 'השמירה נכשלה');
    }
  };

  const busy = create.isPending || update.isPending;

  return (
    <>
      <div className="space-y-5">
        <div className="flex items-start gap-4">
          <CategoryIcon icon={icon} color={color} size="lg" />
          <div className="flex-1 space-y-3">
            <div className="space-y-1.5">
              <Label htmlFor="category-name">שם</Label>
              <Input
                id="category-name"
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder='למשל: רבנים בחו"ל'
                data-testid="category-name"
                autoFocus
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="category-description">תיאור</Label>
              <Input
                id="category-description"
                value={description}
                onChange={(event) => setDescription(event.target.value)}
                placeholder="שורה אחת שמסבירה מי כאן"
              />
            </div>
          </div>
        </div>

        <div className="space-y-2">
          <Label>אייקון</Label>
          <div className="flex flex-wrap gap-1.5">
            {CATEGORY_ICON_KEYS.map((key) => {
              const Glyph = CATEGORY_ICONS[key]!;
              const selected = key === icon;
              return (
                <button
                  key={key}
                  type="button"
                  aria-label={key}
                  aria-pressed={selected}
                  onClick={() => setIcon(key)}
                  className={cn(
                    'inline-flex size-9 items-center justify-center rounded-md border transition-colors hover:bg-accent',
                    selected && 'border-primary bg-accent',
                  )}
                >
                  <Glyph className="size-4" />
                </button>
              );
            })}
          </div>
        </div>

        <div className="space-y-2">
          <Label>צבע</Label>
          <div className="flex flex-wrap gap-2">
            {CATEGORY_COLORS.map((swatch) => (
              <button
                key={swatch}
                type="button"
                aria-label={swatch}
                aria-pressed={swatch === color}
                onClick={() => setColor(swatch)}
                className={cn(
                  'size-7 rounded-full border-2 border-transparent transition-transform hover:scale-110',
                  swatch === color && 'border-foreground',
                )}
                style={{ backgroundColor: swatch }}
              />
            ))}
          </div>
        </div>

        <div className="flex items-center justify-between rounded-md border p-3">
          <div>
            <p className="text-sm font-medium">להציג במסך הבית</p>
            <p className="text-xs text-muted-foreground">אריח עם מספר אנשי הקשר, לניווט מהיר</p>
          </div>
          <Switch
            checked={showOnHome}
            onCheckedChange={setShowOnHome}
            aria-label="להציג במסך הבית"
          />
        </div>

        <div className="space-y-3 rounded-md border p-3">
          <div className="flex items-center justify-between">
            <div>
              <p className="text-sm font-medium">קטגוריה חכמה</p>
              <p className="text-xs text-muted-foreground">
                כלל שממלא את הקטגוריה מעצמו. תמיד אפשר לצרף או להוציא מישהו ביד.
              </p>
            </div>
            <Switch
              checked={smart}
              onCheckedChange={setSmart}
              aria-label="קטגוריה חכמה"
              data-testid="category-smart"
            />
          </div>

          {smart ? (
            <div className="space-y-3">
              <div className="flex items-center gap-2 text-sm">
                <span>מי שמקיים</span>
                <Select value={match} onValueChange={(value) => setMatch(value as 'all' | 'any')}>
                  <SelectTrigger className="w-40" aria-label="אופן השילוב">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="all">את כל התנאים</SelectItem>
                    <SelectItem value="any">לפחות תנאי אחד</SelectItem>
                  </SelectContent>
                </Select>
              </div>

              <div className="space-y-2">
                {rows.map((row, index) => (
                  <ConditionRow
                    key={row.key}
                    row={row}
                    index={index}
                    onChange={(patch) => updateRow(row.key, patch)}
                    onRemove={() => setRows((current) => current.filter((r) => r.key !== row.key))}
                  />
                ))}
              </div>

              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => setRows((current) => [...current, newRow()])}
              >
                <Plus className="size-4" aria-hidden />
                תנאי נוסף
              </Button>

              <div className="rounded-md bg-muted/50 p-3 text-sm" data-testid="category-preview">
                {rule == null ? (
                  <span className="text-muted-foreground">
                    הוסיפו תנאי עם ערך כדי לראות מי יתאים.
                  </span>
                ) : preview.data ? (
                  <>
                    <span className="numeric font-medium">{preview.data.count}</span> אנשי קשר
                    מתאימים עכשיו
                    {preview.data.sample.length > 0 ? (
                      <span className="text-muted-foreground">
                        {' '}
                        — {preview.data.sample.map((contact) => contact.displayName).join(', ')}
                        {preview.data.count > preview.data.sample.length ? '…' : ''}
                      </span>
                    ) : null}
                  </>
                ) : (
                  <span className="text-muted-foreground">בודק…</span>
                )}
              </div>
            </div>
          ) : null}
        </div>
      </div>

      <DialogFooter className="gap-2">
        <Button type="button" variant="ghost" onClick={onClose}>
          ביטול
        </Button>
        <Button type="button" onClick={save} disabled={busy} data-testid="category-save">
          {category ? 'שמירה' : 'יצירה'}
        </Button>
      </DialogFooter>
    </>
  );
}

function ConditionRow({
  row,
  index,
  onChange,
  onRemove,
}: {
  row: Row;
  index: number;
  onChange: (patch: Partial<Row>) => void;
  onRemove: () => void;
}) {
  const valueless = VALUELESS_OPERATORS.includes(row.op);

  let valueInput: React.ReactNode = null;
  if (!valueless) {
    if (row.field === 'country') {
      valueInput = (
        <Select value={row.text} onValueChange={(text) => onChange({ text })}>
          <SelectTrigger aria-label="מדינה" data-testid={`condition-value-${index}`}>
            <SelectValue placeholder="בחירת מדינה" />
          </SelectTrigger>
          <SelectContent>
            {Object.entries(COUNTRY_NAMES_HE).map(([code, label]) => (
              <SelectItem key={code} value={code}>
                {label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      );
    } else if (row.field === 'relationship') {
      valueInput = (
        <Select value={row.text} onValueChange={(text) => onChange({ text })}>
          <SelectTrigger aria-label="סוג קשר" data-testid={`condition-value-${index}`}>
            <SelectValue placeholder="בחירת סוג קשר" />
          </SelectTrigger>
          <SelectContent>
            {RELATIONSHIP_TYPES.map((type) => (
              <SelectItem key={type} value={type}>
                {RELATIONSHIP_LABELS[type].out}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      );
    } else if (row.op === 'within_days') {
      valueInput = (
        <Input
          type="number"
          min={1}
          max={3650}
          value={row.text}
          onChange={(event) => onChange({ text: event.target.value })}
          placeholder="ימים"
          aria-label="מספר ימים"
          data-testid={`condition-value-${index}`}
        />
      );
    } else if (row.op === 'similar') {
      valueInput = (
        <Textarea
          value={row.text}
          onChange={(event) => onChange({ text: event.target.value })}
          placeholder={FIELD_HINTS.meaning}
          aria-label="משפט"
          rows={2}
          data-testid={`condition-value-${index}`}
        />
      );
    } else {
      valueInput = (
        <Input
          value={row.text}
          onChange={(event) => onChange({ text: event.target.value })}
          placeholder={FIELD_HINTS[row.field] ?? 'ערכים, מופרדים בפסיק'}
          aria-label="ערכים"
          data-testid={`condition-value-${index}`}
        />
      );
    }
  }

  return (
    <div
      className="grid grid-cols-[1fr_1fr_1.6fr_auto] items-start gap-2"
      data-testid="condition-row"
    >
      <Select
        value={row.field}
        onValueChange={(field) => onChange({ field: field as CategoryRuleField })}
      >
        <SelectTrigger aria-label="שדה" data-testid={`condition-field-${index}`}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {CATEGORY_RULE_FIELDS.map((field) => (
            <SelectItem key={field} value={field}>
              {RULE_FIELD_LABELS[field]}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      <Select
        value={row.op}
        onValueChange={(op) =>
          onChange({
            op: op as CategoryRuleOperator,
            ...(VALUELESS_OPERATORS.includes(op as CategoryRuleOperator) ? { text: '' } : {}),
          })
        }
      >
        <SelectTrigger aria-label="תנאי" data-testid={`condition-op-${index}`}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {CATEGORY_FIELD_OPERATORS[row.field].map((op) => (
            <SelectItem key={op} value={op}>
              {RULE_OPERATOR_LABELS[op]}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      <div className={cn(valueless && 'self-center text-sm text-muted-foreground')}>
        {valueless ? '—' : valueInput}
      </div>

      <Button type="button" variant="ghost" size="icon" aria-label="הסרת תנאי" onClick={onRemove}>
        <Trash2 className="size-4" />
      </Button>
    </div>
  );
}
