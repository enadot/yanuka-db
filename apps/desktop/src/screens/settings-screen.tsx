import { Database, DatabaseBackup, FileUp, HardDrive, Info, Tags } from 'lucide-react';
import { Link } from 'react-router-dom';
import { formatDateTime } from '@yanuka/utils';
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Separator,
  TagPill,
} from '@yanuka/ui';
import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { useCategories, useDatabaseStats, useTags } from '../hooks/use-contacts';
import { backupNow, backupStatus, exportContactsCsv, type BackupStatus } from '../lib/desktop-io';
import { useRepository } from '../lib/repository';
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
  const repository = useRepository();
  const [backup, setBackup] = useState<BackupStatus | null>(null);
  const [busy, setBusy] = useState<'backup' | 'export' | null>(null);

  useEffect(() => {
    void backupStatus().then(setBackup);
  }, []);

  const runBackup = async () => {
    setBusy('backup');
    try {
      const target = await backupNow();
      if (target) {
        toast.success(`הגיבוי נשמר: ${target}`);
        setBackup(await backupStatus());
      }
    } catch (error) {
      toast.error(error instanceof Error ? error.message : 'הגיבוי נכשל');
    } finally {
      setBusy(null);
    }
  };

  const runExport = async () => {
    setBusy('export');
    try {
      const target = await exportContactsCsv(repository);
      if (target) {
        toast.success(`הייצוא נשמר: ${target}`);
      }
    } catch (error) {
      toast.error(error instanceof Error ? error.message : 'הייצוא נכשל');
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="mx-auto max-w-3xl space-y-6 p-6">
      <h1 className="text-xl font-semibold">הגדרות</h1>

      {!isLocal ? (
        <Alert>
          <Info className="size-4" />
          <AlertTitle>מצב הדגמה</AlertTitle>
          <AlertDescription>
            האפליקציה פועלת כעת בדפדפן עם נתוני דוגמה בזיכרון. שינויים לא נשמרים בין הפעלות. בגרסת
            שולחן העבודה המידע נשמר במסד נתונים מקומי.
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
            <span>
              {stats?.sync.lastSyncAt ? formatDateTime(stats.sync.lastSyncAt) : 'אין עדיין שרת'}
            </span>
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
            שרת סנכרון עדיין לא הוקם — זו החלטה מכוונת בשלב הזה, לא תקלה ולא עניין של חיבור
            לאינטרנט. כל שינוי נרשם ביומן מקומי, וכשיוקם שרת, כל מה שהצטבר יסונכרן אליו. המערכת
            עובדת במלואה, עם או בלי אינטרנט.
          </p>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <FileUp className="size-4" aria-hidden />
            ייבוא
          </CardTitle>
        </CardHeader>
        <CardContent className="flex items-center justify-between gap-4">
          <p className="text-sm text-muted-foreground">
            ייבוא אנשי קשר מקובץ CSV, ואיתור כפילויות שנוצרו ממקורות שונים.
          </p>
          <div className="flex shrink-0 gap-2">
            <Button asChild variant="outline">
              <Link to="/import">ייבוא מקובץ</Link>
            </Button>
            <Button asChild variant="outline">
              <Link to="/duplicates">איתור כפילויות</Link>
            </Button>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <DatabaseBackup className="size-4" aria-hidden />
            גיבוי וייצוא
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          {isLocal ? (
            <p className="text-sm text-muted-foreground">
              גיבוי אוטומטי נלקח פעם ביום בעת פתיחת התוכנה ונשמר ליד מסד הנתונים.
              {' '}
              גיבוי אחרון:{' '}
              <span data-testid="last-backup">
                {backup?.lastBackupAt ? formatDateTime(backup.lastBackupAt) : 'טרם נלקח'}
              </span>
            </p>
          ) : (
            <p className="text-sm text-muted-foreground">
              בגרסת שולחן העבודה נלקח גיבוי אוטומטי פעם ביום, וניתן לגבות ידנית להתקן חיצוני.
              ייצוא ה־CSV פועל גם כאן.
            </p>
          )}
          <div className="flex flex-wrap gap-2">
            {isLocal ? (
              <Button onClick={() => void runBackup()} disabled={busy !== null}>
                {busy === 'backup' ? 'מגבה…' : 'גיבוי עכשיו…'}
              </Button>
            ) : null}
            <Button
              variant="outline"
              onClick={() => void runExport()}
              disabled={busy !== null}
              data-testid="export-csv"
            >
              {busy === 'export' ? 'מייצא…' : 'ייצוא כל אנשי הקשר ל־CSV'}
            </Button>
          </div>
          <p className="text-xs text-muted-foreground">
            קובץ הייצוא נפתח באקסל ומתייבא חזרה דרך מסך הייבוא ללא הגדרה נוספת.
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

      <p className="text-xs text-muted-foreground">
        מאגר הקשרים · גרסה{' '}
        <span className="numeric" data-testid="app-version">
          {__APP_VERSION__}
        </span>
      </p>
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
