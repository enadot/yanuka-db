import { useCallback, useState } from 'react';
import { Link, NavLink, Outlet, useLocation, useNavigate } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { Database, Home, Plus, Search, Settings, Users } from 'lucide-react';
import { Button, Separator, cn } from '@yanuka/ui';
import { useCommandHotkey } from '../../hooks/use-hotkey';
import { useSyncEvents } from '../../hooks/use-sync-events';
import { syncStatus } from '../../lib/desktop-io';
import { useIsLocalDatabase } from '../../lib/repository';
import { GlobalSearchDialog } from '../search/global-search-dialog';
import { ScreenErrorBoundary } from './screen-error-boundary';
import { SyncIndicator } from './sync-indicator';

const NAV_ITEMS = [
  { to: '/', label: 'חיפוש', icon: Home, end: true },
  { to: '/contacts', label: 'אנשי קשר', icon: Users, end: false },
  { to: '/settings', label: 'הגדרות', icon: Settings, end: false },
] as const;

/**
 * Application chrome, in two shapes for two hands.
 *
 * On a wide screen: a narrow right-hand rail, a thin header and the routed
 * screen. The rail sits on the right because the document is RTL — that is the
 * "start" edge, where a reader's eye lands first. It is deliberately minimal:
 * this is a search tool, and every pixel of permanent navigation is a pixel not
 * spent on results.
 *
 * On a phone, that rail would eat more than half the width, so navigation moves
 * to the bottom of the screen — within reach of a thumb, which is the only part
 * of a phone-sized layout that is not negotiable. It is laid out as the last row
 * of the column rather than pinned over the content, so nothing is ever hidden
 * behind it and no screen needs to know it is there. The one thing it adds over
 * the rail is a mark on הגדרות when a decision is waiting: the rail can afford
 * to show the sync indicator permanently and the bar cannot, and a conflict that
 * is only visible on a screen nobody visits is a conflict nobody answers.
 */
export function AppLayout() {
  const [commandOpen, setCommandOpen] = useState(false);
  const navigate = useNavigate();
  const location = useLocation();
  const isLocal = useIsLocalDatabase();

  const openCommand = useCallback(() => setCommandOpen(true), []);
  useCommandHotkey('KeyK', openCommand);
  useSyncEvents();

  const { data: sync } = useQuery({
    queryKey: ['sync-status'],
    queryFn: syncStatus,
    enabled: isLocal,
  });
  const needsAttention = (sync?.openConflicts ?? 0) > 0;

  return (
    <div className="flex h-full min-h-0 flex-col md:flex-row">
      <nav className="hidden w-60 shrink-0 flex-col gap-1.5 border-e bg-sidebar p-4 md:flex">
        <Link to="/" className="mb-4 flex items-center gap-2.5 px-2 py-1">
          <Database className="size-6 text-muted-foreground" aria-hidden />
          <span className="text-lg font-bold">מאגר הקשרים</span>
        </Link>

        {/*
          The active item is marked three ways at once — background, weight and
          a bar on the start edge — rather than by a slightly darker grey. On a
          screen someone returns to after being interrupted, "where am I" should
          not require comparing two shades.
        */}
        {NAV_ITEMS.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            end={item.end}
            className={({ isActive }) =>
              cn(
                'relative flex items-center gap-3 rounded-lg px-3 py-2.5 text-base transition-colors',
                isActive
                  ? 'bg-sidebar-accent font-bold text-sidebar-accent-foreground before:absolute before:inset-y-1.5 before:start-0 before:w-1 before:rounded-full before:bg-primary'
                  : 'font-medium text-muted-foreground hover:bg-sidebar-accent/60 hover:text-sidebar-accent-foreground',
              )
            }
          >
            <item.icon className="size-5" aria-hidden />
            {item.label}
          </NavLink>
        ))}

        <Separator className="my-3" />

        {/*
          The one creative action in the application, so it is the one filled
          button in the chrome and it says what it makes.
        */}
        <Button
          className="h-12 justify-start gap-2 text-base font-bold"
          onClick={() => navigate('/contacts/new')}
        >
          <Plus className="size-5" aria-hidden />
          איש קשר חדש
        </Button>

        <div className="mt-auto">
          <SyncIndicator />
        </div>
      </nav>

      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        <header className="flex h-16 shrink-0 items-center gap-3 border-b px-4">
          {/* The name earns its place only where the rail is not showing it. */}
          <Link to="/" className="flex shrink-0 items-center md:hidden" aria-label="מאגר הקשרים">
            <Database className="size-6 text-muted-foreground" aria-hidden />
          </Link>
          <Button
            variant="outline"
            // `min-w-0 flex-1`, not `w-full`: a flex item at 100% ignores its
            // siblings and the gap between them, which on a phone pushes the
            // button past the edge of the screen by exactly the width of the
            // logo beside it.
            className="min-w-0 flex-1 justify-start gap-2 text-base font-medium text-muted-foreground md:max-w-md"
            onClick={openCommand}
          >
            <Search className="size-5" aria-hidden />
            <span className="flex-1 truncate text-start">חיפוש מהיר…</span>
            {/* The shortcut is written LTR because it names physical keys, and
                it is hidden where there is no keyboard to press them on. */}
            <kbd className="ltr-inline hidden rounded border bg-muted px-1.5 py-0.5 text-[0.7rem] font-medium md:inline">
              Ctrl + K
            </kbd>
          </Button>
        </header>

        <main className="min-h-0 flex-1 overflow-y-auto">
          {/* Keyed by path so navigating away from a crashed screen recovers. */}
          <ScreenErrorBoundary key={location.pathname}>
            <Outlet />
          </ScreenErrorBoundary>
        </main>
      </div>

      <nav
        aria-label="ניווט"
        className="flex shrink-0 border-t bg-sidebar pb-[env(safe-area-inset-bottom)] md:hidden"
      >
        {NAV_ITEMS.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            end={item.end}
            className={({ isActive }) =>
              cn(
                'relative flex min-h-16 flex-1 flex-col items-center justify-center gap-1 text-xs transition-colors',
                isActive
                  ? 'font-bold text-primary after:absolute after:inset-x-6 after:top-0 after:h-0.5 after:rounded-full after:bg-primary'
                  : 'font-medium text-muted-foreground',
              )
            }
          >
            <span className="relative">
              <item.icon className="size-6" aria-hidden />
              {item.to === '/settings' && needsAttention ? (
                <span
                  className="absolute -end-1 -top-0.5 size-2.5 rounded-full bg-amber-500 ring-2 ring-sidebar"
                  aria-label="ממתינה הכרעה"
                />
              ) : null}
            </span>
            {item.label}
          </NavLink>
        ))}

        <button
          type="button"
          onClick={() => navigate('/contacts/new')}
          className="flex min-h-16 flex-1 flex-col items-center justify-center gap-1 text-xs font-bold text-primary"
        >
          <span className="flex size-6 items-center justify-center rounded-full bg-primary text-primary-foreground">
            <Plus className="size-4" aria-hidden />
          </span>
          איש קשר חדש
        </button>
      </nav>

      <GlobalSearchDialog open={commandOpen} onOpenChange={setCommandOpen} />
    </div>
  );
}
