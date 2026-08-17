import { useCallback, useState } from 'react';
import { Link, NavLink, Outlet, useNavigate } from 'react-router-dom';
import { Database, Home, Plus, Search, Settings, Users } from 'lucide-react';
import { Button, Separator, cn } from '@yanuka/ui';
import { useCommandHotkey } from '../../hooks/use-hotkey';
import { GlobalSearchDialog } from '../search/global-search-dialog';
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

  const openCommand = useCallback(() => setCommandOpen(true), []);
  useCommandHotkey('KeyK', openCommand);

  return (
    <div className="flex h-full min-h-0">
      <nav className="flex w-52 shrink-0 flex-col gap-1 border-e bg-sidebar p-3">
        <Link to="/" className="mb-4 flex items-center gap-2 px-2 py-1">
          <Database className="size-5 text-muted-foreground" aria-hidden />
          <span className="font-semibold">מאגר הקשרים</span>
        </Link>

        {NAV_ITEMS.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            end={item.end}
            className={({ isActive }) =>
              cn(
                'flex items-center gap-2 rounded-md px-2 py-2 text-sm transition-colors',
                isActive
                  ? 'bg-sidebar-accent font-medium text-sidebar-accent-foreground'
                  : 'text-muted-foreground hover:bg-sidebar-accent/60 hover:text-sidebar-accent-foreground',
              )
            }
          >
            <item.icon className="size-4" aria-hidden />
            {item.label}
          </NavLink>
        ))}

        <Separator className="my-3" />

        <Button size="sm" className="justify-start gap-2" onClick={() => navigate('/contacts/new')}>
          <Plus className="size-4" aria-hidden />
          איש קשר חדש
        </Button>

        <div className="mt-auto">
          <SyncIndicator />
        </div>
      </nav>

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-14 shrink-0 items-center gap-3 border-b px-4">
          <Button
            variant="outline"
            size="sm"
            className="w-full max-w-md justify-start gap-2 text-muted-foreground"
            onClick={openCommand}
          >
            <Search className="size-4" aria-hidden />
            <span className="flex-1 text-start">חיפוש מהיר…</span>
            {/* The shortcut is written LTR because it names physical keys. */}
            <kbd className="ltr-inline rounded border bg-muted px-1.5 py-0.5 text-[0.7rem] font-medium">
              Ctrl + K
            </kbd>
          </Button>
        </header>

        <main className="min-h-0 flex-1 overflow-y-auto">
          <Outlet />
        </main>
      </div>

      <GlobalSearchDialog open={commandOpen} onOpenChange={setCommandOpen} />
    </div>
  );
}
