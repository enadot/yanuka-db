import { Badge } from './ui/badge.js';
import { cn } from '../lib/utils.js';

/**
 * A tag, rendered from the shadcn Badge primitive.
 *
 * Tags carry a user-chosen colour, applied as a tint rather than a fill so a
 * screen showing a dozen of them still reads as a document rather than a
 * dashboard.
 */
export interface TagPillProps {
  name: string;
  color?: string | null;
  onClick?: () => void;
  className?: string;
}

export function TagPill({ name, color, onClick, className }: TagPillProps) {
  const interactive = onClick != null;

  return (
    <Badge
      variant="secondary"
      asChild={interactive}
      className={cn('font-normal', interactive && 'cursor-pointer hover:opacity-80', className)}
      style={
        color
          ? {
              // 18% alpha keeps the label legible in both themes without
              // needing a second colour per tag.
              backgroundColor: `${color}2e`,
              color,
              borderColor: `${color}55`,
            }
          : undefined
      }
    >
      {interactive ? (
        <button type="button" onClick={onClick}>
          {name}
        </button>
      ) : (
        <span>{name}</span>
      )}
    </Badge>
  );
}
