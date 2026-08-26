import { Component, type ErrorInfo, type ReactNode } from 'react';
import { TriangleAlert } from 'lucide-react';
import { Alert, AlertDescription, AlertTitle, Button } from '@yanuka/ui';

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
}

/**
 * Catches a render error in the routed screen and shows it, instead of React
 * unmounting the whole tree into a blank window.
 *
 * On a desktop machine with no devtools open, a white screen carries zero
 * information — this boundary is the difference between a bug report that says
 * "מסך לבן" and one that names the broken screen and the exact error. The data
 * is safe either way: by the time a screen renders, any mutation has already
 * committed in SQLite.
 */
export class ScreenErrorBoundary extends Component<Props, State> {
  override state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  override componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('screen render error', error, info.componentStack);
  }

  override render() {
    if (!this.state.error) {
      return this.props.children;
    }

    return (
      <div className="mx-auto max-w-xl p-8">
        <Alert variant="destructive">
          <TriangleAlert className="size-4" aria-hidden />
          <AlertTitle>המסך נתקל בשגיאה</AlertTitle>
          <AlertDescription className="space-y-3">
            <p>הנתונים שמורים; זו שגיאת תצוגה בלבד.</p>
            <p className="break-all font-mono text-xs" dir="ltr">
              {this.state.error.message}
            </p>
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                this.setState({ error: null });
                window.location.hash = '#/';
              }}
            >
              חזרה למסך הראשי
            </Button>
          </AlertDescription>
        </Alert>
      </div>
    );
  }
}
