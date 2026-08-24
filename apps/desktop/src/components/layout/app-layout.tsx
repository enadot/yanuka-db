import { useCallback, useState } from 'react';
import { Link, NavLink, Outlet, useLocation, useNavigate } from 'react-router-dom';
import { Database, Home, Plus, Search, Settings, Users } from 'lucide-react';
import { Button, Separator, cn } from '@yanuka/ui';
import { useCommandHotkey } from '../../hooks/use-hotkey';
import { GlobalSearchDialog } from '../search/global-search-dialog';
import { ScreenErrorBoundary } from './screen-error-boundary';
import { SyncIndicator } from './sync-indicator';

const NAV_ITEMS = [
  { to: '/', label: 'חיפוש', icon: Home, end: true },
  { to: '/contacts', label: 'אנשי קשר', icon: Users, end: false },
  { to: '/settings', label: 'הגדרות', icon: Settings, end: false },
] as const;

/**
 * Application chrome: a narrow right-hand rail, a thin header and the routed
 * screen.
 *
 * The rail sits on the right because the document is RTL — that is the "start"
 * edge, where a reader's eye lands first. It is deliberately minimal: this is a
 * search tool, and every pixel of permanent navigation is a pixel not spent on
 * results.
 */
export function AppLayout() {
  const [commandOpen, setCommandOpen] = useState(false);
  const navigate = useNavigate();
  const location = useLocation();

  const openCommand = useCallback(() => setCommandOpen(true), []);
  useCommandHotkey('KeyK', openCommand);

  return (
    <div className="flex h-full min-h-0">
      <nav className="flex w-60 shrink-0 flex-col gap-1.5 border-e bg-sidebar p-4">
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

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-16 shrink-0 items-center gap-3 border-b px-4">
          <Button
            variant="outline"
            className="w-full max-w-md justify-start gap-2 text-base font-medium text-muted-foreground"
            onClick={openCommand}
          >
            <Search className="size-5" aria-hidden />
            <span className="flex-1 text-start">חיפוש מהיר…</span>
            {/* The shortcut is written LTR because it names physical keys. */}
            <kbd className="ltr-inline rounded border bg-muted px-1.5 py-0.5 text-[0.7rem] font-medium">
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

      <GlobalSearchDialog open={commandOpen} onOpenChange={setCommandOpen} />
    </div>
  );
}
