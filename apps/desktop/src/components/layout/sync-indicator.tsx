import { CloudOff, HardDrive, RefreshCw, TriangleAlert } from 'lucide-react';
import { Link } from 'react-router-dom';
import { formatRelative } from '@yanuka/utils';
import { Tooltip, TooltipContent, TooltipTrigger } from '@yanuka/ui';
import { useSyncOverview } from '../../hooks/use-contacts';
import { useIsLocalDatabase } from '../../lib/repository';

/**
 * Offline / sync status.
 *
 * Written in plain language on purpose. The person using this works offline for
 * days at a time and does not need to think about mutation queues — they need
 * to know that their data is safe locally, whether it has reached the server,
 * and whether anything needs their decision. Technical detail stays in the
 * tooltip.
 */
export function SyncIndicator() {
  const { data: sync } = useSyncOverview();
  const isLocal = useIsLocalDatabase();

  const pending = sync?.pendingMutations ?? 0;
  const conflicts = sync?.openConflicts ?? 0;

  let line: string;
  let Icon = HardDrive;
  if (!sync?.configured) {
    line = 'סנכרון: לא הוגדר שרת';
  } else if (sync.syncing) {
    line = 'מסנכרן…';
    Icon = RefreshCw;
  } else if (!sync.online) {
    line = 'השרת אינו זמין כרגע';
    Icon = CloudOff;
  } else {
    line = sync.lastSyncAt ? `מסונכרן · ${formatRelative(sync.lastSyncAt)}` : 'מסונכרן';
  }

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div
          className="space-y-1 rounded-md border bg-background/60 p-2 text-xs"
          data-testid="sync-indicator"
        >
          <div className="flex items-center gap-1.5 font-medium">
            <HardDrive className="size-3.5 text-emerald-600" aria-hidden />
            מאגר מקומי: זמין
          </div>
          <Link
            to={sync?.configured ? '/settings' : '/settings'}
            className="flex items-center gap-1.5 text-muted-foreground hover:text-foreground"
            data-testid="sync-line"
          >
            {Icon !== HardDrive ? <Icon className="size-3.5" aria-hidden /> : null}
            {line}
          </Link>
          {pending > 0 ? (
            <div className="text-muted-foreground">
              <span className="numeric">{pending}</span>{' '}
              {sync?.configured ? 'שינויים ממתינים לשליחה' : 'שינויים רשומים ביומן'}
            </div>
          ) : null}
          {conflicts > 0 ? (
            <Link
              to="/conflicts"
              className="flex items-center gap-1.5 font-medium text-amber-700 hover:underline"
              data-testid="sync-conflicts-link"
            >
              <TriangleAlert className="size-3.5" aria-hidden />
              <span className="numeric">{conflicts}</span> התנגשויות לטיפול
            </Link>
          ) : null}
        </div>
      </TooltipTrigger>
      <TooltipContent side="left" className="max-w-64">
        {!isLocal
          ? 'הרצה במצב הדגמה: הנתונים נטענים לזיכרון בלבד ולא נשמרים.'
          : !sync?.configured
            ? 'המידע נשמר במסד נתונים מקומי במחשב זה, ללא תלות באינטרנט. כדי לסנכרן עם מכשירים נוספים, מצמדים את המחשב לשרת בהגדרות; עד אז כל שינוי נרשם ביומן וישלח כשיוקם שרת.'
            : sync.lastError
              ? `הסנכרון האחרון לא הצליח: ${sync.lastError}. העבודה נמשכת כרגיל, והשינויים יישלחו כשהשרת יחזור.`
              : 'המידע נשמר מקומית תמיד, ונשלח לשרת ברקע. כשהשרת אינו זמין העבודה נמשכת כרגיל, וכל מה שהצטבר נשלח כשהוא חוזר.'}
      </TooltipContent>
    </Tooltip>
  );
}
