import { useEffect, useState } from 'react';

/** Below this width the side rail gives way to a bottom bar (ADR-040). */
export const MOBILE_QUERY = '(max-width: 767px)';

/**
 * Whether the viewport is phone-sized right now. Media-query driven rather
 * than platform driven: the phone layout is also the right one for a narrow
 * desktop window, and the browser build can exercise it in tests.
 */
export function useIsMobile(): boolean {
  const [mobile, setMobile] = useState(() =>
    typeof window !== 'undefined' ? window.matchMedia(MOBILE_QUERY).matches : false,
  );
  useEffect(() => {
    const query = window.matchMedia(MOBILE_QUERY);
    const update = () => setMobile(query.matches);
    update();
    query.addEventListener('change', update);
    return () => query.removeEventListener('change', update);
  }, []);
  return mobile;
}
