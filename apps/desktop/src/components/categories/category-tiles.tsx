import { Link } from 'react-router-dom';
import { LayoutGrid } from 'lucide-react';
import { useCategories } from '../../hooks/use-contacts';
import { CategoryIcon } from './category-icon';

/**
 * The shelves, as tiles above the search box's starting points. Only
 * categories that asked to be here and are not empty appear: an empty tile
 * is a dead end, and the dashboard is one click away for the rest.
 */
export function CategoryTiles() {
  const { data: categories = [] } = useCategories();
  const tiles = categories.filter((category) => category.showOnHome && category.count > 0);

  if (tiles.length === 0) return null;

  return (
    <section className="space-y-2" data-testid="home-categories">
      <div className="flex items-center justify-between">
        <h2 className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
          <LayoutGrid className="size-4" aria-hidden />
          קטגוריות
        </h2>
        <Link to="/categories" className="text-xs text-muted-foreground hover:underline">
          כל הקטגוריות
        </Link>
      </div>
      <div className="grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-4">
        {tiles.map((category) => (
          <Link
            key={category.id}
            to={`/categories/${category.id}`}
            className="flex items-center gap-3 rounded-lg border bg-card p-3 transition-colors hover:bg-accent"
            data-testid="home-category-tile"
          >
            <CategoryIcon icon={category.icon} color={category.color} />
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm font-medium">{category.name}</p>
              <p className="numeric text-xs text-muted-foreground">{category.count}</p>
            </div>
          </Link>
        ))}
      </div>
    </section>
  );
}
