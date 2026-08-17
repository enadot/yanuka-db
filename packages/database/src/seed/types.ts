import type {
  AliasKind,
  CountryCode,
  EmailKind,
  LanguageCode,
  OrganizationKind,
  PhoneKind,
  RelationshipType,
} from '@yanuka/types';

/**
 * Shape of the demo dataset.
 *
 * Contacts reference organizations, tags, categories and one another by a
 * short human-readable `key` rather than by ULID, so the file stays readable
 * and diffable. IDs are minted deterministically when the seed is loaded.
 */
export interface SeedPhone {
  kind: PhoneKind;
  raw: string;
  isPrimary?: boolean;
  label?: string;
}

export interface SeedEmail {
  kind: EmailKind;
  address: string;
  isPrimary?: boolean;
}

export interface SeedAlias {
  kind: AliasKind;
  value: string;
  languageCode?: LanguageCode;
}

export interface SeedContact {
  key: string;
  displayName: string;
  firstName?: string;
  lastName?: string;
  prefix?: string;
  title?: string;
  country?: CountryCode;
  region?: string;
  city?: string;
  address?: string;
  profession?: string;
  role?: string;
  specialties?: string[];
  languages?: LanguageCode[];
  aliases?: SeedAlias[];
  phones?: SeedPhone[];
  emails?: SeedEmail[];
  tags?: string[];
  categories?: string[];
  organizations?: Array<{ key: string; role?: string; isPrimary?: boolean }>;
  notes?: string;
  reasonForSaving?: string;
  source?: string;
  introducedBy?: string;
  isFavorite?: boolean;
}

export interface SeedOrganization {
  key: string;
  name: string;
  kind: OrganizationKind;
  city?: string;
  country?: CountryCode;
  notes?: string;
}

export interface SeedRelationship {
  from: string;
  to: string;
  type: RelationshipType;
  notes?: string;
}

export interface SeedTag {
  name: string;
  color?: string;
}

export interface SeedCategory {
  name: string;
  description?: string;
}

export interface SeedDataset {
  tags: SeedTag[];
  categories: SeedCategory[];
  organizations: SeedOrganization[];
  contacts: SeedContact[];
  relationships: SeedRelationship[];
}
