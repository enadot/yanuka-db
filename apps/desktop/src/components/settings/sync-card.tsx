import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Check, Copy, Link2, Link2Off, RefreshCw, Server } from 'lucide-react';
import { toast } from 'sonner';
import {
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Input,
  Label,
  Separator,
} from '@yanuka/ui';
import { formatDateTime } from '@yanuka/utils';
import {
  syncConnect,
  syncDisconnect,
  syncNow,
  syncShareCode,
  syncStatus,
  type SyncOutcome,
} from '../../lib/desktop-io';
import { useIsLocalDatabase } from '../../lib/repository';

/**
 * Connecting this machine to the other ones.
 *
 * Written for somebody who does not know what a server is and should not have
 * to. There is one thing to paste and one button to press; the words "cursor",
 * "mutation" and "endpoint" appear nowhere. What it does say plainly is the
 * part that actually affects the user's decisions: the archive works completely
 * without any of this, and the passphrase inside the code cannot be recovered.
 */
export function SyncCard() {
  const isLocal = useIsLocalDatabase();
  const queryClient = useQueryClient();

  const [code, setCode] = useState('');
  const [deviceName, setDeviceName] = useState('המחשב שלי');
  const [secret, setSecret] = useState('');
  const [shared, setShared] = useState<string | null>(null);

  const { data: status } = useQuery({ queryKey: ['sync-status'], queryFn: syncStatus });

  /** Everything a sync touches — which is nearly everything on screen. */
  const refreshEverything = async () => {
    await queryClient.invalidateQueries();
  };

  const describe = (outcome: SyncOutcome) => {
    const parts: string[] = [];
    if (outcome.pushed) parts.push(`${outcome.pushed} נשלחו`);
    if (outcome.applied) parts.push(`${outcome.applied} התקבלו`);
    if (!parts.length) return 'הכול מעודכן';
    return parts.join(' · ');
  };

  const connect = useMutation({
    mutationFn: () => syncConnect(code.trim(), deviceName.trim() || 'מחשב'),
    onSuccess: async (outcome) => {
      setCode('');
      toast.success(`המכשיר חובר. ${describe(outcome)}`);
      await refreshEverything();
    },
    onError: (error: unknown) =>
      toast.error(error instanceof Error ? error.message : String(error)),
  });

  const now = useMutation({
    mutationFn: syncNow,
    onSuccess: async (outcome) => {
      if (outcome.conflicts > 0) {
        // Never buried in a success message: a conflict means two versions of
        // something a person wrote, and both are still being kept.
        toast.warning(
          `${describe(outcome)}. ${outcome.conflicts} שינויים הגיעו בגרסה שונה מזו שכאן — שתי הגרסאות נשמרו.`,
        );
      } else {
        toast.success(describe(outcome));
      }
      await refreshEverything();
    },
    onError: (error: unknown) =>
      toast.error(error instanceof Error ? error.message : String(error)),
  });

  const disconnect = useMutation({
    mutationFn: syncDisconnect,
    onSuccess: async () => {
      toast.success('המכשיר נותק. אנשי הקשר נשארו במקומם.');
      await refreshEverything();
    },
  });

  const makeCode = useMutation({
    mutationFn: () => syncShareCode(secret.trim()),
    onSuccess: (value) => {
      setShared(value);
      setSecret('');
    },
    onError: (error: unknown) =>
      toast.error(error instanceof Error ? error.message : String(error)),
  });

  if (!isLocal) {
    return (
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <Server className="size-4" aria-hidden />
            סנכרון בין מכשירים
          </CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-sm text-muted-foreground">
            במצב הדגמה אין מסד נתונים מקומי, ולכן אין מה לסנכרן.
          </p>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <Server className="size-4" aria-hidden />
          סנכרון בין מכשירים
        </CardTitle>
      </CardHeader>

      <CardContent className="space-y-4 text-sm">
        {!status?.connected ? (
          <>
            <p className="text-muted-foreground">
              המאגר עובד במלואו על המחשב הזה בלי שום חיבור. הסנכרון נועד רק כדי שאותם אנשי קשר יופיעו
              גם במחשב אחר ובטלפון.
            </p>

            <div className="space-y-2">
              <Label htmlFor="sync-code">קוד חיבור</Label>
              <Input
                id="sync-code"
                dir="ltr"
                placeholder="yanuka1_…"
                value={code}
                onChange={(event) => setCode(event.target.value)}
              />
              <p className="text-xs text-muted-foreground">
                מתקבל מהמחשב שכבר מחובר, דרך &rdquo;הוספת מכשיר&ldquo;.
              </p>
            </div>

            <div className="space-y-2">
              <Label htmlFor="sync-device-name">שם המכשיר הזה</Label>
              <Input
                id="sync-device-name"
                value={deviceName}
                onChange={(event) => setDeviceName(event.target.value)}
              />
            </div>

            <Button
              type="button"
              className="w-full"
              disabled={!code.trim() || connect.isPending}
              onClick={() => connect.mutate()}
            >
              <Link2 className="size-4" aria-hidden />
              {connect.isPending ? 'מתחבר…' : 'חיבור'}
            </Button>
          </>
        ) : (
          <>
            <div className="flex justify-between">
              <span className="text-muted-foreground">שרת</span>
              <span dir="ltr" className="truncate ps-2 text-xs">
                {status.serverUrl}
              </span>
            </div>
            <Separator />
            <div className="flex justify-between">
              <span className="text-muted-foreground">סנכרון אחרון</span>
              <span>{status.lastSyncAt ? formatDateTime(status.lastSyncAt) : 'טרם בוצע'}</span>
            </div>
            <Separator />
            <div className="flex justify-between">
              <span className="text-muted-foreground">ממתין לשליחה</span>
              <span className="numeric">{status.pendingChanges}</span>
            </div>
            {status.openConflicts > 0 ? (
              <>
                <Separator />
                <div className="flex justify-between text-amber-600">
                  <span>שינויים בשתי גרסאות</span>
                  <span className="numeric">{status.openConflicts}</span>
                </div>
              </>
            ) : null}

            <Button
              type="button"
              className="w-full"
              disabled={now.isPending}
              onClick={() => now.mutate()}
            >
              <RefreshCw className={`size-4 ${now.isPending ? 'animate-spin' : ''}`} aria-hidden />
              {now.isPending ? 'מסנכרן…' : 'סנכרון עכשיו'}
            </Button>

            <Separator />

            <div className="space-y-2">
              <Label htmlFor="sync-secret">הוספת מכשיר נוסף</Label>
              <p className="text-xs text-muted-foreground">
                יש להזין את סיסמת השרת — זו שנקבעה בעת ההתקנה. המחשב הזה אינו שומר אותה, כדי שגניבה
                שלו לא תאפשר לחבר מכשירים נוספים.
              </p>
              <div className="flex gap-2">
                <Input
                  id="sync-secret"
                  dir="ltr"
                  type="password"
                  value={secret}
                  onChange={(event) => setSecret(event.target.value)}
                />
                <Button
                  type="button"
                  variant="outline"
                  disabled={!secret.trim() || makeCode.isPending}
                  onClick={() => makeCode.mutate()}
                >
                  יצירת קוד
                </Button>
              </div>

              {shared ? (
                <div className="space-y-2 rounded-md border bg-muted/40 p-3">
                  <p className="text-xs text-muted-foreground">
                    להעתיק ולהדביק במכשיר החדש. הקוד מכיל את המפתח לפענוח המידע — יש להתייחס אליו
                    כמו למאגר עצמו.
                  </p>
                  <code dir="ltr" className="block break-all text-xs">
                    {shared}
                  </code>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={() => {
                      void navigator.clipboard.writeText(shared);
                      toast.success('הקוד הועתק');
                    }}
                  >
                    <Copy className="size-4" aria-hidden />
                    העתקה
                  </Button>
                </div>
              ) : null}
            </div>

            <Separator />

            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="text-destructive"
              disabled={disconnect.isPending}
              onClick={() => disconnect.mutate()}
            >
              <Link2Off className="size-4" aria-hidden />
              ניתוק המכשיר מהשרת
            </Button>
            <p className="text-xs text-muted-foreground">
              <Check className="me-1 inline size-3" aria-hidden />
              אנשי הקשר נשארים במחשב. גם מה שייעשה אחרי הניתוק יישלח אם יחובר שוב.
            </p>
          </>
        )}
      </CardContent>
    </Card>
  );
}
