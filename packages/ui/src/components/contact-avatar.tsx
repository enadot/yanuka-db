import { Avatar, AvatarFallback } from './ui/avatar.js';
import { cn } from '../lib/utils.js';

/**
 * Initials avatar, composed from the shadcn Avatar primitive.
 *
 * There are no photographs in this database, so the fallback is the whole
 * component. The tint is derived from the name rather than stored, which gives
 * each person a stable colour across sessions and devices without adding a
 * field that would have to sync.
 */
const TINTS = [
  'bg-sky-100 text-sky-900 dark:bg-sky-950 dark:text-sky-200',
  'bg-emerald-100 text-emerald-900 dark:bg-emerald-950 dark:text-emerald-200',
  'bg-amber-100 text-amber-900 dark:bg-amber-950 dark:text-amber-200',
  'bg-violet-100 text-violet-900 dark:bg-violet-950 dark:text-violet-200',
  'bg-rose-100 text-rose-900 dark:bg-rose-950 dark:text-rose-200',
  'bg-teal-100 text-teal-900 dark:bg-teal-950 dark:text-teal-200',
] as const;

function tintFor(seed: string): string {
  let hash = 0;
  for (let i = 0; i < seed.length; i += 1) {
    hash = (hash * 31 + seed.charCodeAt(i)) >>> 0;
  }
  return TINTS[hash % TINTS.length]!;
}

export interface ContactAvatarProps {
  name: string;
  initials: string;
  className?: string;
  size?: 'sm' | 'md' | 'lg';
}

const SIZES = {
  sm: 'size-8 text-xs',
  md: 'size-10 text-sm',
  lg: 'size-16 text-xl',
} as const;

export function ContactAvatar({ name, initials, className, size = 'md' }: ContactAvatarProps) {
  return (
    <Avatar className={cn(SIZES[size], className)}>
      <AvatarFallback className={cn('font-medium', tintFor(name))} aria-label={name}>
        {initials}
      </AvatarFallback>
    </Avatar>
  );
}
