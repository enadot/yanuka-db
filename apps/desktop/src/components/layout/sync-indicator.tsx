import { HardDrive } from 'lucide-react';
import { formatRelative } from '@yanuka/utils';
import { Tooltip, TooltipContent, TooltipTrigger } from '@yanuka/ui';
import { useDatabaseStats } from '../../hooks/use-contacts';
import { useIsLocalDatabase } from '../../lib/repository';

/**
 * Offline / sync status.
 *
 * Written in plain language on purpose. The person using this works offline for
 * days at a time and does not need to think about mutation queues — they need
 * to know that their data is safe locally and how much has yet to leave the
 * machine. Technical detail stays in the tooltip.
 */
export function SyncIndicator() {
  const { data } = useDatabaseStats();
  const isLocal = useIsLocalDatabase();

  const pending = data?.sync.pendingMutations ?? 0;

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div className="space-y-1 rounded-md border bg-background/60 p-2 text-xs">
          <div className="flex items-center gap-1.5 font-medium">
            <HardDrive className="size-3.5 text-emerald-600" aria-hidden />
            מאגר מקומי: זמין
          </div>
          <div className="text-muted-foreground">
            {data?.sync.lastSyncAt
              ? `סנכרון אחרון: ${formatRelative(data.sync.lastSyncAt)}`
              : 'סנכרון: עדיין לא הוקם שרת'}
          </div>
          {pending > 0 ? (
            <div className="text-muted-foreground">
              <span className="numeric">{pending}</span> שינויים רשומים ביומן
            </div>
          ) : null}
        </div>
      </TooltipTrigger>
      <TooltipContent side="left" className="max-w-64">
        {isLocal
          ? 'המידע נשמר במסד נתונים מקומי במחשב זה, ללא תלות באינטרנט. שרת סנכרון עדיין לא הוקם — כל שינוי נרשם ביומן מקומי, וכשיוקם שרת הכול יסונכרן אליו.'
          : 'הרצה במצב הדגמה: הנתונים נטענים לזיכרון בלבד ולא נשמרים.'}
      </TooltipContent>
    </Tooltip>
  );
}
