import type { ComponentType } from 'react';
import {
  Baby,
  BookOpen,
  Briefcase,
  Building,
  Coins,
  Crown,
  Folder,
  Gavel,
  Gem,
  Globe,
  GraduationCap,
  Hammer,
  HandCoins,
  Handshake,
  Heart,
  House,
  Landmark,
  Lightbulb,
  MapPin,
  Megaphone,
  Music,
  Newspaper,
  NotebookPen,
  PhoneOff,
  Plane,
  Scale,
  Scroll,
  Shield,
  Sparkles,
  Star,
  Stethoscope,
  Tag,
  Truck,
  Users,
  Utensils,
  Wand2,
  Wrench,
} from 'lucide-react';
import type { CategoryMembershipKind } from '@yanuka/types';
import { Badge, cn } from '@yanuka/ui';

/**
 * The fixed icon set a category may wear. Keys are what the database stores
 * (`categories.icon`), so the list can only grow, never be renamed.
 */
export const CATEGORY_ICONS: Record<string, ComponentType<{ className?: string }>> = {
  globe: Globe,
  landmark: Landmark,
  scroll: Scroll,
  building: Building,
  'hand-coins': HandCoins,
  handshake: Handshake,
  wrench: Wrench,
  stethoscope: Stethoscope,
  scale: Scale,
  sparkles: Sparkles,
  'phone-off': PhoneOff,
  'notebook-pen': NotebookPen,
  users: Users,
  megaphone: Megaphone,
  briefcase: Briefcase,
  'graduation-cap': GraduationCap,
  star: Star,
  heart: Heart,
  'book-open': BookOpen,
  house: House,
  'map-pin': MapPin,
  shield: Shield,
  gem: Gem,
  music: Music,
  truck: Truck,
  gavel: Gavel,
  tag: Tag,
  utensils: Utensils,
  baby: Baby,
  plane: Plane,
  newspaper: Newspaper,
  hammer: Hammer,
  coins: Coins,
  crown: Crown,
  lightbulb: Lightbulb,
  folder: Folder,
};

export const CATEGORY_ICON_KEYS = Object.keys(CATEGORY_ICONS);

/** A dozen tints that read well as both a tile and a pill, in either theme. */
export const CATEGORY_COLORS = [
  '#2563eb',
  '#7c3aed',
  '#b45309',
  '#0f766e',
  '#16a34a',
  '#ea580c',
  '#78716c',
  '#dc2626',
  '#475569',
  '#0891b2',
  '#9f1239',
  '#a16207',
];

const DEFAULT_COLOR = '#64748b';

const SIZES = {
  sm: { box: 'size-6', icon: 'size-3.5' },
  md: { box: 'size-9', icon: 'size-4' },
  lg: { box: 'size-14', icon: 'size-7' },
} as const;

export interface CategoryIconProps {
  icon: string | null;
  color: string | null;
  size?: keyof typeof SIZES;
  className?: string;
}

/** The category's glyph in a tinted circle. */
export function CategoryIcon({ icon, color, size = 'md', className }: CategoryIconProps) {
  const Glyph = (icon && CATEGORY_ICONS[icon]) || Folder;
  const tint = color ?? DEFAULT_COLOR;
  return (
    <span
      className={cn(
        'inline-flex shrink-0 items-center justify-center rounded-full',
        SIZES[size].box,
        className,
      )}
      style={{ backgroundColor: `${tint}22`, color: tint }}
      aria-hidden
    >
      <Glyph className={SIZES[size].icon} />
    </span>
  );
}

export interface CategoryPillProps {
  name: string;
  icon: string | null;
  color: string | null;
  /** Shown as a small wand when the person is here by rule. */
  membership?: CategoryMembershipKind;
  onClick?: () => void;
  className?: string;
}

/** A category as a pill — on cards, in lists, in the settings summary. */
export function CategoryPill({
  name,
  icon,
  color,
  membership,
  onClick,
  className,
}: CategoryPillProps) {
  const Glyph = (icon && CATEGORY_ICONS[icon]) || Folder;
  const tint = color ?? DEFAULT_COLOR;
  const interactive = onClick != null;
  const body = (
    <>
      <Glyph className="size-3" aria-hidden />
      {name}
      {membership === 'rule' ? <Wand2 className="size-3 opacity-70" aria-label="לפי הכלל" /> : null}
    </>
  );

  return (
    <Badge
      variant="secondary"
      asChild={interactive}
      title={
        membership === 'rule'
          ? 'שויך אוטומטית לפי הכלל'
          : membership === 'manual'
            ? 'שויך ידנית'
            : undefined
      }
      className={cn(
        'gap-1 font-normal',
        interactive && 'cursor-pointer hover:opacity-80',
        className,
      )}
      style={{ backgroundColor: `${tint}2e`, color: tint, borderColor: `${tint}55` }}
    >
      {interactive ? (
        <button type="button" onClick={onClick}>
          {body}
        </button>
      ) : (
        <span>{body}</span>
      )}
    </Badge>
  );
}
