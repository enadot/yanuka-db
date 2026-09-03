import { useEffect, useState } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import { useForm, useFieldArray } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { ArrowRight, Plus, Trash2, TriangleAlert } from 'lucide-react';
import { toast } from 'sonner';
import { ContactInputSchema } from '@yanuka/validation';
import type { z } from 'zod';
import { PHONE_KINDS } from '@yanuka/types';
import type { ContactInput, DuplicateCandidate } from '@yanuka/core';
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Badge,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Checkbox,
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  Input,
  Label,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Separator,
  Skeleton,
  Textarea,
} from '@yanuka/ui';
import { useContact, useCreateContact, useTags, useUpdateContact } from '../hooks/use-contacts';
import { useRepository } from '../lib/repository';
import { useDebouncedValue } from '../hooks/use-debounced-value';

const PHONE_KIND_LABELS: Record<string, string> = {
  mobile: 'נייד',
  office: 'משרד',
  home: 'בית',
  whatsapp: 'וואטסאפ',
  fax: 'פקס',
  assistant: 'מזכירות',
  other: 'אחר',
};

/**
 * What the form holds while it is being edited.
 *
 * Distinct from `ContactInput`: the schema applies defaults, so its *input*
 * type has optional fields while its *output* type has all of them. The form
 * state is the former and the submit handler receives the latter, which is why
 * `useForm` is given both.
 */
type ContactFormValues = z.input<typeof ContactInputSchema>;

const EMPTY: ContactInput = {
  firstName: null,
  lastName: null,
  displayName: '',
  prefix: null,
  title: null,
  country: null,
  region: null,
  city: null,
  address: null,
  postalCode: null,
  profession: null,
  role: null,
  notes: null,
  reasonForSaving: null,
  source: null,
  introducedBy: null,
  introducedByContactId: null,
  isFavorite: false,
  phones: [],
  emails: [],
  aliases: [],
  specialties: [],
  languages: [],
  tagIds: [],
  categoryIds: [],
  organizations: [],
};

export interface ContactEditScreenProps {
  mode: 'create' | 'edit';
}

/**
 * Create and edit a contact.
 *
 * Only the name is required. That is a product decision, not laziness: the
 * records worth the most here are the vague ones — "the electrician from
 * Antwerp, ask Adler" — and a form that insists on a phone number or a
 * structured first/last name would push exactly those out of the database.
 */
export function ContactEditScreen({ mode }: ContactEditScreenProps) {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const repository = useRepository();
  const { data: existing, isLoading } = useContact(mode === 'edit' ? id : undefined);
  const { data: tags = [] } = useTags();
  const createContact = useCreateContact();
  const updateContact = useUpdateContact();

  const form = useForm<ContactFormValues, unknown, ContactInput>({
    resolver: zodResolver(ContactInputSchema),
    defaultValues: EMPTY,
  });

  const phones = useFieldArray({ control: form.control, name: 'phones' });

  useEffect(() => {
    if (mode !== 'edit' || !existing) return;
    form.reset({
      ...EMPTY,
      firstName: existing.firstName,
      lastName: existing.lastName,
      displayName: existing.displayName,
      prefix: existing.prefix,
      title: existing.title,
      country: existing.country,
      region: existing.region,
      city: existing.city,
      address: existing.address,
      postalCode: existing.postalCode,
      profession: existing.profession,
      role: existing.role,
      notes: existing.notes,
      reasonForSaving: existing.reasonForSaving,
      source: existing.source,
      introducedBy: existing.introducedBy,
      isFavorite: existing.isFavorite,
      phones: existing.phones.map((phone) => ({
        id: phone.id,
        kind: phone.kind,
        raw: phone.raw,
        label: phone.label,
        isPrimary: phone.isPrimary,
      })),
      specialties: existing.specialties,
      tagIds: existing.tags.map((tag) => tag.id),
      // Only hand-pinned categories are part of the form; rule membership is
      // derived and an edit must not turn it into a pin.
      categoryIds: existing.categories
        .filter((category) => category.membership === 'manual')
        .map((category) => category.id),
    });
  }, [existing, form, mode]);

  // Duplicate check runs while typing rather than on submit, so the warning
  // arrives before the user has invested effort in a record that already exists.
  const watchedName = form.watch('displayName');
  const watchedPhones = form.watch('phones');
  const debouncedName = useDebouncedValue(watchedName ?? '', 400);
  const [duplicates, setDuplicates] = useState<DuplicateCandidate[]>([]);

  // `watch` returns a fresh array on every render, so depending on it directly
  // would re-run the duplicate lookup continuously. The numbers themselves are
  // what the check actually reads, so a joined string is the stable dependency.
  const phoneKey = (watchedPhones ?? []).map((phone) => phone.raw).join('|');

  useEffect(() => {
    if (mode === 'edit' || debouncedName.trim().length < 2) {
      setDuplicates([]);
      return;
    }
    let cancelled = false;
    // The form holds the schema's *input* shape, where the defaulted fields are
    // still optional. Fill them in before crossing the repository boundary.
    const phonesForCheck = phoneKey
      .split('|')
      .filter(Boolean)
      .map((raw) => ({ kind: 'mobile' as const, raw, label: null, isPrimary: false }));

    void repository
      .findDuplicates({ displayName: debouncedName, phones: phonesForCheck }, id)
      .then((found) => {
        if (!cancelled) setDuplicates(found);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [debouncedName, phoneKey, mode, repository, id]);

  const onSubmit = async (values: ContactInput) => {
    try {
      if (mode === 'create') {
        const created = await createContact.mutateAsync(values);
        toast.success('איש הקשר נוסף');
        navigate(`/contacts/${created.id}`);
      } else if (id) {
        await updateContact.mutateAsync({
          id,
          patch: values,
          // Optimistic concurrency: if the record changed elsewhere since the
          // form was opened, the write is refused rather than silently winning.
          baseVersion: existing?.version,
        });
        toast.success('השינויים נשמרו');
        navigate(`/contacts/${id}`);
      }
    } catch (error) {
      const message =
        error && typeof error === 'object' && 'message' in error
          ? String((error as { message: unknown }).message)
          : 'השמירה נכשלה';
      toast.error(message);
    }
  };

  if (mode === 'edit' && isLoading) {
    return (
      <div className="mx-auto max-w-3xl space-y-4 p-6">
        <Skeleton className="h-10 w-64" />
        <Skeleton className="h-96 w-full" />
      </div>
    );
  }

  const selectedTags = form.watch('tagIds') ?? [];

  return (
    <div className="mx-auto max-w-3xl space-y-4 p-6">
      <Button asChild variant="ghost" size="sm" className="-ms-2 gap-1">
        <Link to={mode === 'edit' && id ? `/contacts/${id}` : '/contacts'}>
          <ArrowRight className="size-4" aria-hidden />
          חזרה
        </Link>
      </Button>

      <h1 className="text-xl font-semibold">
        {mode === 'create' ? 'איש קשר חדש' : `עריכת ${existing?.displayName ?? ''}`}
      </h1>

      {duplicates.length > 0 ? (
        <Alert>
          <TriangleAlert className="size-4" />
          <AlertTitle>ייתכן שאיש הקשר כבר קיים</AlertTitle>
          <AlertDescription className="space-y-1">
            {duplicates.map((candidate) => (
              <Link
                key={candidate.contact.id}
                to={`/contacts/${candidate.contact.id}`}
                className="block hover:underline"
              >
                {candidate.contact.displayName}
                <span className="ms-2 text-xs text-muted-foreground">
                  {candidate.reasons.join(' · ')}
                </span>
              </Link>
            ))}
          </AlertDescription>
        </Alert>
      ) : null}

      <Form {...form}>
        <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle className="text-base">שם</CardTitle>
            </CardHeader>
            <CardContent className="grid gap-4 sm:grid-cols-2">
              <FormField
                control={form.control}
                name="prefix"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>תואר</FormLabel>
                    <FormControl>
                      <Input placeholder="הרב, ר', ד״ר" {...field} value={field.value ?? ''} />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <FormField
                control={form.control}
                name="displayName"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>שם מלא *</FormLabel>
                    <FormControl>
                      <Input placeholder="שם האדם" {...field} />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <FormField
                control={form.control}
                name="firstName"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>שם פרטי</FormLabel>
                    <FormControl>
                      <Input {...field} value={field.value ?? ''} />
                    </FormControl>
                  </FormItem>
                )}
              />

              <FormField
                control={form.control}
                name="lastName"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>שם משפחה</FormLabel>
                    <FormControl>
                      <Input {...field} value={field.value ?? ''} />
                    </FormControl>
                  </FormItem>
                )}
              />
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="flex-row items-center justify-between">
              <CardTitle className="text-base">טלפונים</CardTitle>
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() =>
                  phones.append({
                    kind: 'mobile',
                    raw: '',
                    label: null,
                    isPrimary: phones.fields.length === 0,
                  })
                }
              >
                <Plus className="size-4" aria-hidden />
                הוספת מספר
              </Button>
            </CardHeader>
            <CardContent className="space-y-2">
              {phones.fields.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                  לא הוזנו מספרים. אפשר לשמור איש קשר גם בלי מספר טלפון.
                </p>
              ) : null}

              {phones.fields.map((field, index) => (
                <div key={field.id} className="flex items-end gap-2">
                  <FormField
                    control={form.control}
                    name={`phones.${index}.kind`}
                    render={({ field: kindField }) => (
                      <FormItem className="w-32">
                        <FormLabel className="text-xs">סוג</FormLabel>
                        <Select value={kindField.value} onValueChange={kindField.onChange}>
                          <FormControl>
                            <SelectTrigger>
                              <SelectValue />
                            </SelectTrigger>
                          </FormControl>
                          <SelectContent>
                            {PHONE_KINDS.map((kind) => (
                              <SelectItem key={kind} value={kind}>
                                {PHONE_KIND_LABELS[kind]}
                              </SelectItem>
                            ))}
                          </SelectContent>
                        </Select>
                      </FormItem>
                    )}
                  />

                  <FormField
                    control={form.control}
                    name={`phones.${index}.raw`}
                    render={({ field: rawField }) => (
                      <FormItem className="flex-1">
                        <FormLabel className="text-xs">מספר</FormLabel>
                        <FormControl>
                          {/* Typed exactly as remembered; normalization happens
                              on save and never rewrites what was entered. */}
                          <Input className="numeric" placeholder="054-000-0000" {...rawField} />
                        </FormControl>
                        <FormMessage />
                      </FormItem>
                    )}
                  />

                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    aria-label="הסרת מספר"
                    onClick={() => phones.remove(index)}
                  >
                    <Trash2 className="size-4 text-destructive" />
                  </Button>
                </div>
              ))}
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle className="text-base">מקום ומקצוע</CardTitle>
            </CardHeader>
            <CardContent className="grid gap-4 sm:grid-cols-2">
              <FormField
                control={form.control}
                name="city"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>עיר</FormLabel>
                    <FormControl>
                      <Input placeholder="ירושלים" {...field} value={field.value ?? ''} />
                    </FormControl>
                  </FormItem>
                )}
              />

              <FormField
                control={form.control}
                name="country"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>מדינה</FormLabel>
                    <FormControl>
                      <Input
                        placeholder="IL"
                        maxLength={2}
                        className="ltr-inline uppercase"
                        {...field}
                        value={field.value ?? ''}
                        onChange={(event) =>
                          field.onChange(event.target.value.toUpperCase() || null)
                        }
                      />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <FormField
                control={form.control}
                name="profession"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>מקצוע</FormLabel>
                    <FormControl>
                      <Input placeholder='סופר סת"ם' {...field} value={field.value ?? ''} />
                    </FormControl>
                  </FormItem>
                )}
              />

              <FormField
                control={form.control}
                name="role"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>תפקיד</FormLabel>
                    <FormControl>
                      <Input placeholder="ראש ישיבה" {...field} value={field.value ?? ''} />
                    </FormControl>
                  </FormItem>
                )}
              />
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle className="text-base">תגיות</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="flex flex-wrap gap-3">
                {tags.map((tag) => {
                  const checked = selectedTags.includes(tag.id);
                  return (
                    <div key={tag.id} className="flex items-center gap-1.5">
                      <Checkbox
                        id={`tag-${tag.id}`}
                        checked={checked}
                        onCheckedChange={(next) =>
                          form.setValue(
                            'tagIds',
                            next
                              ? [...selectedTags, tag.id]
                              : selectedTags.filter((candidate) => candidate !== tag.id),
                          )
                        }
                      />
                      <Label htmlFor={`tag-${tag.id}`} className="cursor-pointer font-normal">
                        {tag.name}
                      </Label>
                    </div>
                  );
                })}
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle className="text-base">הערות</CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <FormField
                control={form.control}
                name="notes"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>הערה חופשית</FormLabel>
                    <FormControl>
                      <Textarea
                        rows={5}
                        placeholder="כל פרט שיעזור למצוא את האדם בעתיד — מי הכיר, מה הוא עושה, במה אפשר להיעזר בו"
                        {...field}
                        value={field.value ?? ''}
                      />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <FormField
                control={form.control}
                name="reasonForSaving"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>נשמר בגלל</FormLabel>
                    <FormControl>
                      <Input
                        placeholder="הסיבה שבגללה שמרנו את איש הקשר"
                        {...field}
                        value={field.value ?? ''}
                      />
                    </FormControl>
                  </FormItem>
                )}
              />

              <FormField
                control={form.control}
                name="introducedBy"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>מי הכיר</FormLabel>
                    <FormControl>
                      <Input placeholder="שם הממליץ" {...field} value={field.value ?? ''} />
                    </FormControl>
                  </FormItem>
                )}
              />
            </CardContent>
          </Card>

          <Separator />

          <div className="flex items-center gap-2">
            <Button type="submit" disabled={form.formState.isSubmitting}>
              {mode === 'create' ? 'הוספת איש קשר' : 'שמירת שינויים'}
            </Button>
            <Button asChild type="button" variant="ghost">
              <Link to={mode === 'edit' && id ? `/contacts/${id}` : '/contacts'}>ביטול</Link>
            </Button>
            {form.formState.errors.displayName ? (
              <Badge variant="destructive" className="ms-auto font-normal">
                יש להזין שם
              </Badge>
            ) : null}
          </div>
        </form>
      </Form>
    </div>
  );
}
