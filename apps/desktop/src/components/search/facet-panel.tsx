import type { FacetField, FacetFilters, Facets } from '@yanuka/types';
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
  Badge,
  Button,
  Checkbox,
  Label,
} from '@yanuka/ui';

const FACET_LABELS: Record<FacetField, string> = {
  country: 'מדינה',
  city: 'עיר',
  profession: 'מקצוע',
  specialty: 'התמחות',
  tag: 'תגית',
  category: 'קטגוריה',
  organization: 'מוסד',
  language: 'שפה',
};

/** Order the panel presents facets in — broadest geography first. */
const FACET_ORDER: FacetField[] = [
  'country',
  'city',
  'profession',
  'specialty',
  'tag',
  'category',
  'organization',
  'language',
];

export interface FacetPanelProps {
  facets: Facets;
  filters: FacetFilters;
  onChange: (filters: FacetFilters) => void;
}

/**
 * Facet filters for the current result set.
 *
 * This is what turns "18 results" into an answer. Rather than making the user
 * refine their words, it shows them the shape of what they already found —
 * nine in Israel, four in the US — and lets them cut it down by clicking.
 *
 * Counts reflect the currently filtered set, so selecting a country will show
 * the other countries at zero. That is a known simplification; see
 * docs/SEARCH.md.
 */
export function FacetPanel({ facets, filters, onChange }: FacetPanelProps) {
  const activeCount = Object.values(filters).reduce(
    (total, values) => total + (values?.length ?? 0),
    0,
  );

  const toggle = (field: FacetField, value: string) => {
    const current = filters[field] ?? [];
    const next = current.includes(value)
      ? current.filter((candidate) => candidate !== value)
      : [...current, value];

    const updated = { ...filters };
    if (next.length === 0) delete updated[field];
    else updated[field] = next;
    onChange(updated);
  };

  const available = FACET_ORDER.filter((field) => (facets[field]?.length ?? 0) > 0);
  if (available.length === 0) return null;

  return (
    <aside className="w-60 shrink-0 space-y-3">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-medium">צמצום תוצאות</h2>
        {activeCount > 0 ? (
          <Button variant="ghost" size="sm" className="h-7 text-xs" onClick={() => onChange({})}>
            נקה ({activeCount})
          </Button>
        ) : null}
      </div>

      <Accordion
        type="multiple"
        // Geography and occupation are the two the user almost always wants;
        // the rest stay collapsed so the panel does not become a wall.
        defaultValue={['country', 'profession']}
        className="w-full"
      >
        {available.map((field) => (
          <AccordionItem key={field} value={field}>
            <AccordionTrigger className="py-2 text-sm">
              {FACET_LABELS[field]}
              {filters[field]?.length ? (
                <Badge variant="secondary" className="ms-auto me-2">
                  {filters[field]!.length}
                </Badge>
              ) : null}
            </AccordionTrigger>
            <AccordionContent className="space-y-1.5 pb-3">
              {facets[field]!.map((facet) => {
                const id = `${field}-${facet.value}`;
                const checked = filters[field]?.includes(facet.value) ?? false;
                return (
                  <div key={facet.value} className="flex items-center gap-2">
                    <Checkbox
                      id={id}
                      checked={checked}
                      onCheckedChange={() => toggle(field, facet.value)}
                    />
                    <Label htmlFor={id} className="flex-1 cursor-pointer text-sm font-normal">
                      {facet.label}
                    </Label>
                    <span className="numeric text-xs text-muted-foreground">{facet.count}</span>
                  </div>
                );
              })}
            </AccordionContent>
          </AccordionItem>
        ))}
      </Accordion>
    </aside>
  );
}
