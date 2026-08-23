import { useEffect } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import {
  ArrowRight,
  Building2,
  MessageCircle,
  Pencil,
  Phone,
  Star,
  Trash2,
  UserX,
} from 'lucide-react';
import { formatFullName, formatSubtitle, initials } from '@yanuka/core';
import { countryName, formatDateTime, languageName, telHref, whatsappHref } from '@yanuka/utils';
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
  ContactAvatar,
  EmptyState,
  FieldRow,
  Separator,
  Skeleton,
  TagPill,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@yanuka/ui';
import { toast } from 'sonner';
import { useContact, useDeleteContact, useSetFavorite } from '../hooks/use-contacts';
import { NoteComposer } from '../components/contact/note-composer';
import { RelationshipEditor } from '../components/contact/relationship-editor';
import { useRepository } from '../lib/repository';

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
 * Everything known about one person.
 *
 * The ordering is deliberate and follows what someone reaching for this record
 * actually needs: how to contact them, then who they are, then — crucially —
 * the free text and the relationships, which is where the value of an archive
 * like this really sits.
 */
export function ContactDetailScreen() {
  const { id } = useParams<{ id: string }>();
  const { data: contact, isLoading } = useContact(id);
  const repository = useRepository();
  const navigate = useNavigate();
  const setFavorite = useSetFavorite();
  const deleteContact = useDeleteContact();

  // Records the visit so recency can inform ranking and the home screen.
  useEffect(() => {
    if (id) void repository.touchContact(id).catch(() => undefined);
  }, [id, repository]);

  if (isLoading) {
    return (
      <div className="mx-auto max-w-4xl space-y-4 p-6">
        <Skeleton className="h-24 w-full" />
        <Skeleton className="h-64 w-full" />
      </div>
    );
  }

  if (!contact) {
    return (
      <div className="mx-auto max-w-4xl p-6">
        <EmptyState
          icon={<UserX />}
          title="איש הקשר לא נמצא"
          description="ייתכן שהרשומה נמחקה או שהקישור אינו תקין."
          action={
            <Button asChild variant="outline">
              <Link to="/contacts">חזרה לרשימה</Link>
            </Button>
          }
        />
      </div>
    );
  }

  const remove = async () => {
    await deleteContact.mutateAsync(contact.id);
    // Soft delete, so offering to undo is honest — the row is still there.
    toast.success('איש הקשר הועבר לסל המחזור', {
      action: {
        label: 'ביטול',
        onClick: () => {
          void repository.restoreContact(contact.id);
        },
      },
    });
    navigate('/contacts');
  };

  return (
    <div className="mx-auto max-w-4xl space-y-6 p-6">
      <Button asChild variant="ghost" size="sm" className="-ms-2 gap-1">
        <Link to="/contacts">
          <ArrowRight className="size-4" aria-hidden />
          חזרה לרשימה
        </Link>
      </Button>

      <header className="flex items-start gap-4">
        <ContactAvatar
          size="lg"
          name={contact.displayName}
          initials={initials(contact.displayName)}
        />

        <div className="min-w-0 flex-1 space-y-1">
          <h1 className="text-2xl font-semibold">{formatFullName(contact)}</h1>
          <p className="text-muted-foreground">{formatSubtitle(contact)}</p>
          <div className="flex flex-wrap gap-1.5 pt-1">
            {contact.tags.map((tag) => (
              <TagPill key={tag.id} name={tag.name} color={tag.color} />
            ))}
            {contact.categories.map((category) => (
              <Badge key={category.id} variant="outline" className="font-normal">
                {category.name}
              </Badge>
            ))}
          </div>
        </div>

        <div className="flex shrink-0 items-center gap-1">
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                aria-label={contact.isFavorite ? 'הסרה ממועדפים' : 'הוספה למועדפים'}
                onClick={() =>
                  setFavorite.mutate({ id: contact.id, isFavorite: !contact.isFavorite })
                }
              >
                <Star
                  className={contact.isFavorite ? 'size-4 fill-amber-400 text-amber-400' : 'size-4'}
                />
              </Button>
            </TooltipTrigger>
            <TooltipContent>{contact.isFavorite ? 'הסרה ממועדפים' : 'סימון כמועדף'}</TooltipContent>
          </Tooltip>

          <Button asChild variant="outline" size="sm">
            <Link to={`/contacts/${contact.id}/edit`}>
              <Pencil className="size-4" aria-hidden />
              עריכה
            </Link>
          </Button>

          <AlertDialog>
            <AlertDialogTrigger asChild>
              <Button variant="ghost" size="icon" aria-label="מחיקה">
                <Trash2 className="size-4 text-destructive" />
              </Button>
            </AlertDialogTrigger>
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>למחוק את {contact.displayName}?</AlertDialogTitle>
                <AlertDialogDescription>
                  הרשומה תועבר לסל המחזור ולא תופיע בחיפוש. המידע נשמר וניתן לשחזר אותו.
                </AlertDialogDescription>
              </AlertDialogHeader>
              <AlertDialogFooter>
                <AlertDialogCancel>ביטול</AlertDialogCancel>
                <AlertDialogAction onClick={remove}>מחיקה</AlertDialogAction>
              </AlertDialogFooter>
            </AlertDialogContent>
          </AlertDialog>
        </div>
      </header>

      {contact.phones.length > 0 || contact.emails.length > 0 ? (
        <Card>
          <CardHeader>
            <CardTitle className="text-base">דרכי התקשרות</CardTitle>
          </CardHeader>
          <CardContent className="space-y-2">
            {contact.phones.map((phone) => (
              <div key={phone.id} className="flex items-center gap-3">
                <Badge variant="secondary" className="w-20 justify-center font-normal">
                  {PHONE_KIND_LABELS[phone.kind] ?? phone.kind}
                </Badge>
                {/* The number as written down, kept LTR so its punctuation
                    renders correctly inside the RTL document. */}
                <a href={telHref(phone)} className="numeric flex-1 hover:underline">
                  {phone.raw}
                </a>
                {phone.label ? (
                  <span className="text-xs text-muted-foreground">{phone.label}</span>
                ) : null}
                <div className="flex gap-1">
                  <Button asChild variant="ghost" size="icon" aria-label="חיוג">
                    <a href={telHref(phone)}>
                      <Phone className="size-4" />
                    </a>
                  </Button>
                  {whatsappHref(phone) ? (
                    <Button asChild variant="ghost" size="icon" aria-label="וואטסאפ">
                      <a href={whatsappHref(phone)!} target="_blank" rel="noreferrer">
                        <MessageCircle className="size-4" />
                      </a>
                    </Button>
                  ) : null}
                </div>
              </div>
            ))}

            {contact.emails.length > 0 && contact.phones.length > 0 ? <Separator /> : null}

            {contact.emails.map((email) => (
              <div key={email.id} className="flex items-center gap-3">
                <Badge variant="secondary" className="w-20 justify-center font-normal">
                  אימייל
                </Badge>
                <a href={`mailto:${email.address}`} className="ltr-inline flex-1 hover:underline">
                  {email.address}
                </a>
              </div>
            ))}
          </CardContent>
        </Card>
      ) : null}

      <Card>
        <CardHeader>
          <CardTitle className="text-base">פרטים</CardTitle>
        </CardHeader>
        <CardContent>
          <dl className="divide-y">
            <FieldRow label="מקצוע">{contact.profession}</FieldRow>
            <FieldRow label="תפקיד">{contact.role}</FieldRow>
            <FieldRow label="התמחויות">
              {contact.specialties.length > 0 ? (
                <div className="flex flex-wrap gap-1">
                  {contact.specialties.map((specialty) => (
                    <Badge key={specialty} variant="outline" className="font-normal">
                      {specialty}
                    </Badge>
                  ))}
                </div>
              ) : null}
            </FieldRow>
            <FieldRow label="מקום">
              {[contact.city, contact.region, countryName(contact.country)]
                .filter(Boolean)
                .join(', ')}
            </FieldRow>
            <FieldRow label="כתובת">{contact.address}</FieldRow>
            <FieldRow label="שמות נוספים">
              {contact.aliases.length > 0
                ? contact.aliases.map((alias) => alias.value).join(' · ')
                : null}
            </FieldRow>
            <FieldRow label="שפות">
              {contact.languages.length > 0
                ? contact.languages.map(languageName).filter(Boolean).join(', ')
                : null}
            </FieldRow>
            <FieldRow label="מקור">{contact.source}</FieldRow>
            <FieldRow label="הכיר לנו">
              {contact.introducedByContactId ? (
                <Link
                  to={`/contacts/${contact.introducedByContactId}`}
                  className="text-primary hover:underline"
                >
                  {contact.introducedBy}
                </Link>
              ) : (
                contact.introducedBy
              )}
            </FieldRow>
          </dl>
        </CardContent>
      </Card>

      <NoteComposer contact={contact} />

      {contact.organizations.length > 0 ? (
        <Card>
          <CardHeader>
            <CardTitle className="text-base">מוסדות וארגונים</CardTitle>
          </CardHeader>
          <CardContent className="space-y-2">
            {contact.organizations.map((link) => (
              <div key={link.id} className="flex items-center gap-3">
                <Building2 className="size-4 shrink-0 text-muted-foreground" aria-hidden />
                <span className="font-medium">{link.organization.name}</span>
                {link.role ? (
                  <Badge variant="secondary" className="font-normal">
                    {link.role}
                  </Badge>
                ) : null}
                <span className="ms-auto text-xs text-muted-foreground">
                  {[link.organization.city, countryName(link.organization.country)]
                    .filter(Boolean)
                    .join(', ')}
                </span>
              </div>
            ))}
          </CardContent>
        </Card>
      ) : null}

      {contact.notes || contact.reasonForSaving ? (
        <Card>
          <CardHeader>
            <CardTitle className="text-base">הערות</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            {contact.notes ? (
              <p className="whitespace-pre-wrap text-sm leading-relaxed">{contact.notes}</p>
            ) : null}
            {contact.reasonForSaving ? (
              <div className="rounded-md border-s-2 border-s-primary bg-muted/40 p-3">
                <p className="mb-1 text-xs font-medium text-muted-foreground">נשמר בגלל</p>
                <p className="text-sm">{contact.reasonForSaving}</p>
              </div>
            ) : null}
          </CardContent>
        </Card>
      ) : null}

      <RelationshipEditor contact={contact} />

      <p className="text-xs text-muted-foreground">
        נוצר {formatDateTime(contact.createdAt)} · עודכן {formatDateTime(contact.updatedAt)} · גרסה{' '}
        <span className="numeric">{contact.version}</span>
      </p>
    </div>
  );
}
