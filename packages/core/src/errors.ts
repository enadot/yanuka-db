/**
 * Every failure the repository layer can produce, as a closed set.
 *
 * The UI has to say something useful in Hebrew for each of these, so they are
 * enumerated rather than left as free-form `Error`s. `conflict` in particular
 * is not an error the user caused — it is the sync engine reporting that two
 * devices edited the same record, and the UI must offer a choice rather than a
 * failure message.
 */
export type RepositoryErrorCode =
  | 'not_found'
  | 'validation'
  | 'conflict'
  | 'stale_version'
  | 'permission_denied'
  | 'duplicate'
  | 'database'
  | 'unavailable';

export class RepositoryError extends Error {
  readonly code: RepositoryErrorCode;
  readonly details: unknown;

  constructor(code: RepositoryErrorCode, message: string, details?: unknown) {
    super(message);
    this.name = 'RepositoryError';
    this.code = code;
    this.details = details ?? null;
  }

  static notFound(what: string): RepositoryError {
    return new RepositoryError('not_found', `${what} לא נמצא`);
  }

  static validation(message: string, details?: unknown): RepositoryError {
    return new RepositoryError('validation', message, details);
  }

  static staleVersion(expected: number, actual: number): RepositoryError {
    return new RepositoryError(
      'stale_version',
      'הרשומה עודכנה במקום אחר. רענן ונסה שוב.',
      { expected, actual },
    );
  }
}

/** Short Hebrew message shown to the user for each failure code. */
export const ERROR_MESSAGES: Record<RepositoryErrorCode, string> = {
  not_found: 'הרשומה לא נמצאה',
  validation: 'חלק מהשדות אינם תקינים',
  conflict: 'נמצאו שתי גרסאות של הרשומה',
  stale_version: 'הרשומה עודכנה במקום אחר',
  permission_denied: 'אין לך הרשאה לפעולה זו',
  duplicate: 'רשומה זהה כבר קיימת',
  database: 'אירעה שגיאה בגישה למאגר המקומי',
  unavailable: 'המאגר אינו זמין כרגע',
};

export function isRepositoryError(value: unknown): value is RepositoryError {
  return value instanceof RepositoryError;
}

/**
 * Coerce an unknown throwable into a RepositoryError.
 *
 * Errors crossing the Tauri IPC boundary arrive as plain objects, not Error
 * instances, so the shape is reconstructed rather than assumed.
 */
export function toRepositoryError(value: unknown): RepositoryError {
  if (isRepositoryError(value)) return value;

  if (typeof value === 'object' && value !== null && 'code' in value) {
    const candidate = value as { code: unknown; message?: unknown; details?: unknown };
    if (typeof candidate.code === 'string' && candidate.code in ERROR_MESSAGES) {
      return new RepositoryError(
        candidate.code as RepositoryErrorCode,
        typeof candidate.message === 'string'
          ? candidate.message
          : ERROR_MESSAGES[candidate.code as RepositoryErrorCode],
        candidate.details,
      );
    }
  }

  return new RepositoryError(
    'database',
    value instanceof Error ? value.message : ERROR_MESSAGES.database,
    value,
  );
}
