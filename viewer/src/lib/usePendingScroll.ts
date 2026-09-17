import { useCallback, useEffect, useRef } from "react";

/** Scrolls to a target once the rows showing it have rendered: `scroll`
 * runs after the next render for `initial`, then for each target passed to
 * the returned function. */
export function usePendingScroll<T>(
  initial: T | null,
  scroll: (target: T) => void,
): (target: T) => void {
  const pending = useRef(initial);
  useEffect(() => {
    if (pending.current === null) return;
    scroll(pending.current);
    pending.current = null;
  });
  return useCallback((target: T) => {
    pending.current = target;
  }, []);
}
