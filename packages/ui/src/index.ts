/**
 * Every shadcn/ui registry component installed in this project, re-exported
 * as one surface.
 *
 * The rule for this codebase is that all UI is built from these primitives —
 * there is no second component library. App-specific pieces (ContactCard,
 * SearchResultRow, …) live alongside them and are composed from them, never
 * hand-rolled. See docs/DECISIONS.md ADR-015.
 */

// -- shadcn/ui registry components -----------------------------------------
export * from './components/ui/accordion.js';
export * from './components/ui/alert.js';
export * from './components/ui/alert-dialog.js';
export * from './components/ui/avatar.js';
export * from './components/ui/badge.js';
export * from './components/ui/breadcrumb.js';
export * from './components/ui/button.js';
export * from './components/ui/card.js';
export * from './components/ui/checkbox.js';
export * from './components/ui/collapsible.js';
export * from './components/ui/command.js';
export * from './components/ui/dialog.js';
export * from './components/ui/dropdown-menu.js';
export * from './components/ui/form.js';
export * from './components/ui/hover-card.js';
export * from './components/ui/input.js';
export * from './components/ui/label.js';
export * from './components/ui/pagination.js';
export * from './components/ui/popover.js';
export * from './components/ui/radio-group.js';
export * from './components/ui/scroll-area.js';
export * from './components/ui/select.js';
export * from './components/ui/separator.js';
export * from './components/ui/sheet.js';
export * from './components/ui/skeleton.js';
export * from './components/ui/sonner.js';
export * from './components/ui/switch.js';
export * from './components/ui/table.js';
export * from './components/ui/tabs.js';
export * from './components/ui/textarea.js';
export * from './components/ui/toggle.js';
export * from './components/ui/toggle-group.js';
export * from './components/ui/tooltip.js';

// -- application composites -------------------------------------------------
export * from './components/contact-avatar.js';
export * from './components/empty-state.js';
export * from './components/field-row.js';
export * from './components/tag-pill.js';

// -- utilities --------------------------------------------------------------
export { cn } from './lib/utils.js';
