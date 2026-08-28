import { useState } from 'react';
import { KeyRound, ShieldAlert } from 'lucide-react';
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Input,
  Label,
} from '@yanuka/ui';
import { unlockDatabase } from '../lib/desktop-io';

/**
 * The one screen that appears instead of the app: an encrypted database whose
 * key the OS credential store does not hold. That happens exactly one way —
 * the file was carried here from another machine (a restored backup after a
 * reinstall) — and the recovery key the user kept off the machine is the way
 * in. See docs/SECURITY.md.
 */
export function UnlockScreen() {
  const [key, setKey] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const unlock = async () => {
    setBusy(true);
    setError(null);
    try {
      await unlockDatabase(key);
      // The simplest correct resume: a clean reload boots straight into the
      // now-open database, with every query starting fresh.
      window.location.reload();
    } catch (raised) {
      const message =
        raised && typeof raised === 'object' && 'message' in raised
          ? String((raised as { message: unknown }).message)
          : 'פתיחת המאגר נכשלה';
      setError(message);
      setBusy(false);
    }
  };

  return (
    <div className="flex min-h-screen items-center justify-center bg-background p-6">
      <Card className="w-full max-w-md">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <ShieldAlert className="size-5" aria-hidden />
            המאגר מוצפן ונעול
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <p className="text-sm text-muted-foreground">
            קובץ המאגר במחשב הזה מוצפן, והמפתח שלו אינו שמור כאן — בדרך כלל כי המאגר שוחזר
            מגיבוי לאחר התקנת Windows מחדש או מעבר למחשב אחר. הזנת מפתח השחזור תפתח את
            המאגר, והמפתח יישמר כדי שהפתיחה הבאה תהיה אוטומטית.
          </p>
          <div className="space-y-2">
            <Label htmlFor="recovery-key">מפתח השחזור</Label>
            <Input
              id="recovery-key"
              dir="ltr"
              autoFocus
              className="text-center font-mono"
              placeholder="XXXXXXXX-XXXXXXXX-…"
              value={key}
              onChange={(event) => setKey(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' && key.trim() && !busy) void unlock();
              }}
              data-testid="recovery-key-input"
            />
          </div>
          {error ? (
            <Alert variant="destructive">
              <AlertTitle>הפתיחה נכשלה</AlertTitle>
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          ) : null}
          <Button
            className="w-full gap-1.5"
            onClick={() => void unlock()}
            disabled={!key.trim() || busy}
            data-testid="unlock-database"
          >
            <KeyRound className="size-4" aria-hidden />
            {busy ? 'פותח…' : 'פתיחת המאגר'}
          </Button>
        </CardContent>
      </Card>
    </div>
  );
}
