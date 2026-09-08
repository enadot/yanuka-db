import { isTauri } from './tauri-repository';

/**
 * Where the bundle is running. Three answers matter to the screens:
 *
 * - a plain browser (development, tests, design review)
 * - the desktop shell on Windows
 * - the Android shell (ADR-040)
 *
 * Layout is *not* decided here — that follows the viewport (`useIsMobile`), so
 * a narrow desktop window gets the phone layout too. This answers only the
 * questions that depend on the platform's abilities: a save dialog exists on
 * the desktop and not on Android; the notebook importer wants a file picker.
 */
export function isAndroidApp(): boolean {
  return isTauri() && /android/i.test(navigator.userAgent);
}

/** The desktop shell: save dialogs, external backups, the embedding model. */
export function isDesktopApp(): boolean {
  return isTauri() && !isAndroidApp();
}
