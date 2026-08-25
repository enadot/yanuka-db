import { useQuery } from '@tanstack/react-query';
import { HardDrive } from 'lucide-react';
import { formatRelative } from '@yanuka/utils';
import { Tooltip, TooltipContent, TooltipTrigger } from '@yanuka/ui';
import { useIsLocalDatabase } from '../../lib/repository';
import { backupStatus, syncStatus } from '../../lib/desktop-io';

/**
 * Where the data stands, in the only terms that are currently true.
 *
 * This has had to change twice, and the reason is worth keeping. It first read
 * "סנכרון אחרון: מעולם לא" with a count of changes waiting — accurate, and read
 * by the user as a stalled queue, because there was no transport and that queue
 * was never going to drain. It was then rewritten to lead with the daily backup,
 * which was the honest answer while sync did not exist. Now it does exist, but
 * only once a device has actually been connected.
 *
 * So the line follows the state rather than the roadmap. Connected: when work
 * last left this machine, and how much has not. Not connected: the backup, which
 * is what protects the archive when nothing else does. Neither says "never" at
 * a user who has done nothing wrong.
 */
export function SyncIndicator() {
  const isLocal = useIsLocalDatabase();

  const { data: sync } = useQuery({
    queryKey: ['sync-status'],
    queryFn: syncStatus,
    enabled: isLocal,
  });
  const { data: backup } = useQuery({
    queryKey: ['backup-status'],
    queryFn: backupStatus,
    enabled: isLocal,
  });

  const pending = sync?.pendingChanges ?? 0;

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div className="space-y-1 rounded-md border bg-background/60 p-2 text-xs">
          <div className="flex items-center gap-1.5 font-medium">
            <HardDrive className="size-3.5 text-emerald-600" aria-hidden />
            מאגר מקומי: זמין
          </div>

          {!isLocal ? (
            <div className="text-amber-600">מצב הדגמה — הנתונים לא נשמרים</div>
          ) : sync?.connected ? (
            <>
              <div className="text-muted-foreground">
                סונכרן: {formatRelative(sync.lastSyncAt ?? null)}
              </div>
              {pending > 0 ? (
                <div className="text-amber-600">{pending} שינויים ממתינים לשליחה</div>
              ) : null}
              {sync.openConflicts > 0 ? (
                <div className="text-amber-600">{sync.openConflicts} שינויים בשתי גרסאות</div>
              ) : null}
            </>
          ) : (
            <div className="text-muted-foreground">
              גיבוי אחרון: {formatRelative(backup?.lastBackupAt ?? null)}
            </div>
          )}
        </div>
      </TooltipTrigger>

      <TooltipContent side="left" className="max-w-72">
        {!isLocal
          ? 'הרצה במצב הדגמה: הנתונים נטענים לזיכרון בלבד ולא נשמרים.'
          : sync?.connected
            ? `המידע נשמר במחשב הזה ועובד גם ללא אינטרנט. שינויים נשלחים למכשירים האחרים כשיש חיבור${
                pending > 0 ? `, וכרגע ${pending} ממתינים` : ''
              }. גיבוי אוטומטי נלקח פעם ביום.`
            : 'המידע נשמר במסד נתונים מקומי במחשב זה ועובד גם ללא אינטרנט, וגיבוי נלקח אוטומטית פעם ביום. סנכרון עם מכשירים נוספים אפשרי דרך ההגדרות, ואינו נדרש.'}
      </TooltipContent>
    </Tooltip>
  );
}
