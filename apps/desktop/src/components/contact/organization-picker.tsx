import { useState } from 'react';
import { Building2, Plus, Trash2 } from 'lucide-react';
import { toast } from 'sonner';
import type { Organization, Ulid } from '@yanuka/types';
import { Badge, Button, Card, CardContent, CardHeader, CardTitle, Input } from '@yanuka/ui';
import { countryName } from '@yanuka/utils';
import { useCreateOrganization, useOrganizations } from '../../hooks/use-contacts';
import { useDebouncedValue } from '../../hooks/use-debounced-value';

/** One membership row as the form holds it. */
export interface OrganizationLinkValue {
  organizationId: Ulid;
  role?: string | null;
  isPrimary?: boolean;
  startedAt?: string | null;
  endedAt?: string | null;
}

export interface OrganizationPickerProps {
  value: OrganizationLinkValue[];
  onChange: (next: OrganizationLinkValue[]) => void;
}

/**
 * Attach a contact to the institutions they belong to.
 *
 * Search-then-create rather than a plain text field: an organization is a
 * record other contacts point at too, so "ישיבת מיר" typed three different ways
 * is three institutions and the archive stops being able to answer "who else is
 * from there". The inline create exists because refusing to save until the user
 * goes and defines the institution first is exactly the friction that keeps a
 * half-remembered detail out of the database.
 */
export function OrganizationPicker({ value, onChange }: OrganizationPickerProps) {
  const [query, setQuery] = useState('');
  const debouncedQuery = useDebouncedValue(query, 250);
  const { data: matches = [] } = useOrganizations(debouncedQuery || undefined);
  const createOrganization = useCreateOrganization();

  const linkedIds = new Set(value.map((link) => link.organizationId));
  const suggestions = matches.filter((organization) => !linkedIds.has(organization.id)).slice(0, 6);
  const trimmed = query.trim();
  const exactMatch = matches.some((organization) => organization.name === trimmed);

  const attach = (organization: Organization) => {
    onChange([
      ...value,
      {
        organizationId: organization.id,
        role: null,
        isPrimary: value.length === 0,
        startedAt: null,
        endedAt: null,
      },
    ]);
    setQuery('');
  };

  const createAndAttach = async () => {
    if (!trimmed) return;
    try {
      const organization = await createOrganization.mutateAsync({
        name: trimmed,
        kind: 'organization',
        city: null,
        region: null,
        country: null,
        address: null,
        notes: null,
      });
      attach(organization);
      toast.success(`המוסד "${organization.name}" נוצר`);
    } catch {
      toast.error('יצירת המוסד נכשלה');
    }
  };

  // Linked rows need the organization's name, and the search results only hold
  // whatever the current query matched. A separate unfiltered read keeps an
  // already-attached institution readable while the user searches for another.
  const { data: allOrganizations = [] } = useOrganizations();
  const nameOf = (id: Ulid) =>
    [...allOrganizations, ...matches].find((organization) => organization.id === id);

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">מוסדות וארגונים</CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        {value.map((link, index) => {
          const organization = nameOf(link.organizationId);
          return (
            <div key={link.organizationId} className="flex items-center gap-2">
              <Building2 className="size-4 shrink-0 text-muted-foreground" aria-hidden />
              <span className="font-medium">{organization?.name ?? link.organizationId}</span>
              {organization?.city || organization?.country ? (
                <span className="text-xs text-muted-foreground">
                  {[organization.city, countryName(organization.country)]
                    .filter(Boolean)
                    .join(', ')}
                </span>
              ) : null}
              <Input
                className="ms-auto w-44"
                placeholder="תפקיד"
                aria-label={`תפקיד ב${organization?.name ?? 'מוסד'}`}
                value={link.role ?? ''}
                onChange={(event) => {
                  const next = [...value];
                  next[index] = { ...link, role: event.target.value || null };
                  onChange(next);
                }}
              />
              <Button
                type="button"
                variant="ghost"
                size="icon"
                aria-label="ניתוק מהמוסד"
                onClick={() => onChange(value.filter((_, position) => position !== index))}
              >
                <Trash2 className="size-4 text-destructive" />
              </Button>
            </div>
          );
        })}

        <Input
          placeholder="חיפוש מוסד — ישיבה, בית כנסת, ארגון"
          aria-label="חיפוש מוסד"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />

        {trimmed ? (
          <div className="flex flex-wrap gap-2">
            {suggestions.map((organization) => (
              <Button
                key={organization.id}
                type="button"
                variant="outline"
                size="sm"
                onClick={() => attach(organization)}
              >
                {organization.name}
                {organization.city ? (
                  <Badge variant="secondary" className="font-normal">
                    {organization.city}
                  </Badge>
                ) : null}
              </Button>
            ))}

            {exactMatch ? null : (
              <Button
                type="button"
                variant="secondary"
                size="sm"
                disabled={createOrganization.isPending}
                onClick={() => void createAndAttach()}
              >
                <Plus className="size-4" aria-hidden />
                {`הוספת מוסד חדש בשם "${trimmed}"`}
              </Button>
            )}
          </div>
        ) : null}
      </CardContent>
    </Card>
  );
}
