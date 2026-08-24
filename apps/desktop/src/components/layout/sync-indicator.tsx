import { useQuery } from '@tanstack/react-query';
import { HardDrive } from 'lucide-react';
import { formatRelative } from '@yanuka/utils';
import { Tooltip, TooltipContent, TooltipTrigger } from '@yanuka/ui';
import { useDatabaseStats } from '../../hooks/use-contacts';
import { useIsLocalDatabase } from '../../lib/repository';
import { backupStatus } from '../../lib/desktop-io';

/**
 * Where the data stands, in the only terms that are currently true.
 *
 * This used to lead with "סנכרון אחרון: מעולם לא" and a count of changes
 * "waiting to sync", and both were read — reasonably — as something being
 * stuck. Nothing is stuck: sync has no transport yet (ADR-019), so that queue
 * was never going to drain and there was no failure to report. Telling a
 * non-technical user that their work is pending, forever, on a product whose
 * first promise is that nothing gets lost, is the worst possible way to be
 * accurate.
 *
 * The line that actually answers "is my work safe" today is the daily backup
 * (ADR-028), so that is the line shown. The mutation log still matters — it is
 * what guarantees the work done before sync exists travels with it — but it
 * belongs in the tooltip, phrased as the safety net it is.
 */
export function SyncIndicator() {
  const { data } = useDatabaseStats();
  const isLocal = useIsLocalDatabase();
  const { data: backup } = useQuery({
    queryKey: ['backup-status'],
    queryFn: backupStatus,
    enabled: isLocal,
  });

  const recorded = data?.sync.pendingMutations ?? 0;

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div className="space-y-1 rounded-md border bg-background/60 p-2 text-xs">
          <div className="flex items-center gap-1.5 font-medium">
            <HardDrive className="size-3.5 text-emerald-600" aria-hidden />
            מאגר מקומי: זמין
          </div>
          {isLocal ? (
            <div className="text-muted-foreground">
              גיבוי אחרון: {formatRelative(backup?.lastBackupAt ?? null)}
            </div>
          ) : (
            <div className="text-amber-600">מצב הדגמה — הנתונים לא נשמרים</div>
          )}
        </div>
      </TooltipTrigger>
      <TooltipContent side="left" className="max-w-72">
        {isLocal
          ? `המידע נשמר במסד נתונים מקומי במחשב זה ועובד גם ללא אינטרנט, וגיבוי נלקח אוטומטית פעם ביום. סנכרון בין מכשירים עדיין לא פעיל; עד שיופעל, כל שינוי נרשם ביומן השינויים (${recorded}) כדי שכל מה שנעשה עד אז יעבור איתו.`
          : 'הרצה במצב הדגמה: הנתונים נטענים לזיכרון בלבד ולא נשמרים.'}
      </TooltipContent>
    </Tooltip>
  );
}
