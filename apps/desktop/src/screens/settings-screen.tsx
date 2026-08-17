import { Database, HardDrive, Info, Tags } from 'lucide-react';
import { formatDateTime } from '@yanuka/utils';
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Separator,
  TagPill,
} from '@yanuka/ui';
import { useCategories, useDatabaseStats, useTags } from '../hooks/use-contacts';
import { useIsLocalDatabase } from '../lib/repository';

/**
 * Settings and database status.
 *
 * Reports what is actually true about this installation rather than offering
 * options that do not exist yet. The sync and permissions sections will appear
 * here when the server lands; until then, saying so plainly is better than a
 * disabled toggle.
 */
export function SettingsScreen() {
  const { data: stats } = useDatabaseStats();
  const { data: tags = [] } = useTags();
  const { data: categories = [] } = useCategories();
  const isLocal = useIsLocalDatabase();

  return (
    <div className="mx-auto max-w-3xl space-y-6 p-6">
      <h1 className="text-xl font-semibold">הגדרות</h1>

      {!isLocal ? (
        <Alert>
          <Info className="size-4" />
          <AlertTitle>מצב הדגמה</AlertTitle>
          <AlertDescription>
            האפליקציה פועלת כעת בדפדפן עם נתוני דוגמה בזיכרון. שינויים לא נשמרים בין הפעלות. בגרסת
            שולחן העבודה המידע נשמר במסד נתונים מקומי מוצפן.
          </AlertDescription>
        </Alert>
      ) : null}

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <HardDrive className="size-4" aria-hidden />
            מאגר מקומי
          </CardTitle>
        </CardHeader>
        <CardContent className="grid grid-cols-2 gap-4 sm:grid-cols-4">
          <Stat label="אנשי קשר" value={stats?.contacts} />
          <Stat label="מוסדות" value={stats?.organizations} />
          <Stat label="קשרים" value={stats?.relationships} />
          <Stat label="הערות" value={stats?.notes} />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <Database className="size-4" aria-hidden />
            סנכרון
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-2 text-sm">
          <div className="flex justify-between">
            <span className="text-muted-foreground">סנכרון אחרון</span>
            <span>{stats?.sync.lastSyncAt ? formatDateTime(stats.sync.lastSyncAt) : 'טרם בוצע'}</span>
          </div>
          <Separator />
          <div className="flex justify-between">
            <span className="text-muted-foreground">שינויים ממתינים</span>
            <span className="numeric">{stats?.sync.pendingMutations ?? 0}</span>
          </div>
          <Separator />
          <div className="flex justify-between">
            <span className="text-muted-foreground">התנגשויות פתוחות</span>
            <span className="numeric">{stats?.sync.openConflicts ?? 0}</span>
          </div>
          <p className="pt-2 text-xs text-muted-foreground">
            כל שינוי נרשם מקומית ומסונכרן כשהחיבור חוזר. המערכת עובדת במלואה גם ללא אינטרנט.
          </p>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <Tags className="size-4" aria-hidden />
            תגיות וקטגוריות
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-2">
            <p className="text-sm text-muted-foreground">
              תגיות (<span className="numeric">{tags.length}</span>)
            </p>
            <div className="flex flex-wrap gap-1.5">
              {tags.map((tag) => (
                <TagPill key={tag.id} name={tag.name} color={tag.color} />
              ))}
            </div>
          </div>
          <Separator />
          <div className="space-y-2">
            <p className="text-sm text-muted-foreground">
              קטגוריות (<span className="numeric">{categories.length}</span>)
            </p>
            <div className="flex flex-wrap gap-1.5">
              {categories.map((category) => (
                <TagPill key={category.id} name={category.name} />
              ))}
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

function Stat({ label, value }: { label: string; value: number | undefined }) {
  return (
    <div>
      <p className="numeric text-2xl font-semibold">{value ?? '—'}</p>
      <p className="text-xs text-muted-foreground">{label}</p>
    </div>
  );
}
