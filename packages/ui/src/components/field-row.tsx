import type { ReactNode } from 'react';
import { cn } from '../lib/utils.js';

/**
 * A labelled value on the contact detail screen.
 *
 * Empty fields are rendered as an em dash rather than hidden: on a record
 * assembled from decades-old notebooks, knowing that a field is *blank* is
 * itself information, and a layout that silently reflows as data arrives is
 * harder to scan.
 */
export interface FieldRowProps {
  label: string;
  children?: ReactNode;
  /** Render `—` when the value is absent. Set false to hide the row entirely. */
  showWhenEmpty?: boolean;
  className?: string;
}

export function FieldRow({ label, children, showWhenEmpty = true, className }: FieldRowProps) {
  const isEmpty = children == null || children === '' || children === false;
  if (isEmpty && !showWhenEmpty) return null;

  return (
    <div className={cn('grid grid-cols-[8rem_1fr] items-baseline gap-3 py-1.5', className)}>
      <dt className="text-sm text-muted-foreground">{label}</dt>
      <dd className="text-sm">{isEmpty ? <span className="text-muted-foreground">—</span> : children}</dd>
    </div>
  );
}
