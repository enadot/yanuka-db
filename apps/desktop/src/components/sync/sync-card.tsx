import { useState } from 'react';
import { CloudOff, Link2, RefreshCw, TriangleAlert, Unplug } from 'lucide-react';
import { Link } from 'react-router-dom';
import { toast } from 'sonner';
import { formatDateTime, formatRelative } from '@yanuka/utils';
import {
  Alert,
  AlertDescription,
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
  AlertTitle,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Input,
  Label,
  Separator,
} from '@yanuka/ui';
import {
  useConnectSync,
  useCreatePairCode,
  useDisconnectSync,
  useSyncNow,
  useSyncOverview,
} from '../../hooks/use-contacts';

/**
 * The settings card for sync between devices (ADR-039).
 *
 * Two faces: an unpaired machine sees the two fields pairing needs, a paired
 * one sees where it stands — last exchange, what is waiting, what needs a
 * decision — and the three things it can do: sync now, admit another device,
 * or leave. Nothing here is required for the application to work; the copy
 * says so.
 */
export function SyncCard() {
  const { data: sync } = useSyncOverview();
  const connect = useConnectSync();
  const disconnect = useDisconnectSync();
  const syncNow = useSyncNow();
  const pairCode = useCreatePairCode();

  const [serverUrl, setServerUrl] = useState('');
  const [code, setCode] = useState('');
  const [deviceName, setDeviceName] = useState('');

  const submit = async () => {
    try {
      const overview = await connect.mutateAsync({
        serverUrl,
        code,
        deviceName: deviceName || undefined,
      });
      if (overview.lastError) {
        toast.warning(`המכשיר צומד, אבל הסנכרון הראשון לא הושלם: ${overview.lastError}`);
      } else {
        toast.success(`המכשיר צומד ל־${overview.serverName ?? 'השרת'} והמאגר סונכרן`);
      }
      setCode('');
    } catch (error) {
      toast.error(error instanceof Error ? error.message : 'הצימוד נכשל');
    }
  };

  const runNow = async () => {
    const overview = await syncNow.mutateAsync();
    if (overview.lastError) {
      toast.error(overview.lastError);
    } else {
      toast.success('המאגר מסונכרן');
    }
  };

  return (
    <Card data-testid="sync-card">
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <Link2 className="size-4" aria-hidden />
          סנכרון בין מכשירים
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        {!sync?.configured ? (
          <>
            <p className="text-sm text-muted-foreground">
              המחשב הזה עובד לבד, וזה בסדר גמור: כל שינוי נרשם ביומן מקומי. כדי לעבוד עם עוד
              מכשיר, מקימים שרת קטן (ראו <code className="ltr-inline">server/README.md</code>)
              ומצמדים אליו את המחשב בקוד חד־פעמי שהשרת מציג.
            </p>
            <div className="grid gap-3 sm:grid-cols-2">
              <div className="space-y-1.5">
                <Label htmlFor="sync-url">כתובת השרת</Label>
                <Input
                  id="sync-url"
                  className="ltr-inline"
                  dir="ltr"
                  placeholder="http://192.168.1.10:8787"
                  value={serverUrl}
                  onChange={(event) => setServerUrl(event.target.value)}
                  data-testid="sync-url"
                />
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="sync-code">קוד צימוד</Label>
                <Input
                  id="sync-code"
                  className="ltr-inline"
                  dir="ltr"
                  placeholder="K7PT-4MXQ"
                  value={code}
                  onChange={(event) => setCode(event.target.value)}
                  data-testid="sync-code"
                />
              </div>
              <div className="space-y-1.5 sm:col-span-2">
                <Label htmlFor="sync-device-name">שם המכשיר הזה (לא חובה)</Label>
                <Input
                  id="sync-device-name"
                  placeholder="המחשב במשרד"
                  value={deviceName}
                  onChange={(event) => setDeviceName(event.target.value)}
                  data-testid="sync-device-name"
                />
              </div>
            </div>
            <Button
              onClick={() => void submit()}
              disabled={connect.isPending || !serverUrl.trim() || !code.trim()}
              data-testid="sync-connect"
            >
              {connect.isPending ? 'מצמד…' : 'צימוד וסנכרון ראשון'}
            </Button>
          </>
        ) : (
          <>
            {sync.lastError ? (
              <Alert data-testid="sync-error">
                <CloudOff className="size-4" />
                <AlertTitle>הסנכרון האחרון לא הצליח</AlertTitle>
                <AlertDescription>
                  {sync.lastError}. העבודה נמשכת כרגיל; מה שהצטבר יישלח כשהשרת יחזור.
                </AlertDescription>
              </Alert>
            ) : null}
            <dl className="space-y-2 text-sm">
              <Row label="שרת">
                <span data-testid="sync-server-name">{sync.serverName ?? 'ללא שם'}</span>
                <span className="ltr-inline ms-2 text-xs text-muted-foreground">
                  {sync.serverUrl}
                </span>
              </Row>
              <Separator />
              <Row label="המכשיר הזה">{sync.deviceName ?? '—'}</Row>
              <Separator />
              <Row label="סנכרון אחרון">
                {sync.lastSyncAt
                  ? `${formatRelative(sync.lastSyncAt)} (${formatDateTime(sync.lastSyncAt)})`
                  : 'עדיין לא'}
              </Row>
              <Separator />
              <Row label="ממתינים לשליחה">
                <span className="numeric">{sync.pendingMutations}</span>
              </Row>
              <Separator />
              <Row label="התנגשויות">
                {sync.openConflicts > 0 ? (
                  <Link
                    to="/conflicts"
                    className="inline-flex items-center gap-1 text-amber-700 underline"
                  >
                    <TriangleAlert className="size-3.5" aria-hidden />
                    <span className="numeric">{sync.openConflicts}</span> ממתינות להחלטה
                  </Link>
                ) : (
                  'אין'
                )}
              </Row>
            </dl>
            <div className="flex flex-wrap gap-2">
              <Button
                onClick={() => void runNow()}
                disabled={syncNow.isPending || sync.syncing}
                data-testid="sync-now"
              >
                <RefreshCw
                  className={syncNow.isPending || sync.syncing ? 'size-4 animate-spin' : 'size-4'}
                  aria-hidden
                />
                {syncNow.isPending || sync.syncing ? 'מסנכרן…' : 'סנכרון עכשיו'}
              </Button>
              <Button
                variant="outline"
                onClick={() =>
                  pairCode.mutate(undefined, {
                    onError: (error) => toast.error(error.message),
                  })
                }
                disabled={pairCode.isPending}
                data-testid="sync-pair-code"
              >
                קוד למכשיר נוסף
              </Button>
              <AlertDialog>
                <AlertDialogTrigger asChild>
                  <Button variant="ghost" className="text-destructive" data-testid="sync-disconnect">
                    <Unplug className="size-4" aria-hidden />
                    ניתוק מהשרת
                  </Button>
                </AlertDialogTrigger>
                <AlertDialogContent>
                  <AlertDialogHeader>
                    <AlertDialogTitle>לנתק את המחשב הזה מהשרת?</AlertDialogTitle>
                    <AlertDialogDescription>
                      כל אנשי הקשר וההיסטוריה נשארים במחשב הזה. רק החיבור נשכח; אפשר לצמד שוב
                      בכל עת בקוד חדש.
                    </AlertDialogDescription>
                  </AlertDialogHeader>
                  <AlertDialogFooter>
                    <AlertDialogCancel>ביטול</AlertDialogCancel>
                    <AlertDialogAction
                      onClick={() => void disconnect.mutateAsync()}
                      data-testid="sync-disconnect-confirm"
                    >
                      ניתוק
                    </AlertDialogAction>
                  </AlertDialogFooter>
                </AlertDialogContent>
              </AlertDialog>
            </div>
            {pairCode.data ? (
              <div
                className="rounded-md border border-dashed bg-muted/40 p-3 text-sm"
                data-testid="pair-code"
              >
                <p className="text-muted-foreground">
                  במכשיר החדש: הגדרות ← סנכרון בין מכשירים ← הזינו את כתובת השרת ואת הקוד:
                </p>
                <p className="ltr-inline mt-1 select-all text-2xl font-semibold tracking-widest">
                  {pairCode.data.code}
                </p>
                <p className="text-xs text-muted-foreground">
                  תקף עד {formatDateTime(pairCode.data.expiresAt)}, לשימוש חד־פעמי.
                </p>
              </div>
            ) : null}
          </>
        )}
      </CardContent>
    </Card>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-baseline justify-between gap-4">
      <dt className="shrink-0 text-muted-foreground">{label}</dt>
      <dd className="min-w-0 text-end">{children}</dd>
    </div>
  );
}
