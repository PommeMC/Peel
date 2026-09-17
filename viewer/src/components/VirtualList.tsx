import {
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type ReactNode,
  type RefObject,
} from "react";

import { useClientSize } from "../lib/useClientSize";

export const ROW_HEIGHT = 28;
/** Rows rendered beyond each edge of the viewport, so fast scrolling doesn't flash. */
const OVERSCAN = 12;

/** The scrolling element for a list of fixed-height `items`, and its scroll
 * state. Items of type `group` are headers that stick while scrolled past. */
export function useVirtualRows(items: readonly { type: string }[]) {
  const count = items.length;
  const groupPositions = useMemo(
    () =>
      items.flatMap((item, position) =>
        item.type === "group" ? [position] : [],
      ),
    [items],
  );
  const viewport = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const { height } = useClientSize(viewport);

  const top = Math.min(Math.floor(scrollTop / ROW_HEIGHT), count - 1);
  const bottom = Math.min(
    Math.floor((scrollTop + height) / ROW_HEIGHT),
    count - 1,
  );
  let sticky: number | undefined;
  for (const position of groupPositions) {
    if (position >= top) break;
    sticky = position;
  }
  if (groupPositions.includes(top)) sticky = undefined;

  function scrollTo(position: number, align: "start" | "nearest" | "center") {
    const element = viewport.current;
    if (!element) return;
    const rowTop = position * ROW_HEIGHT;
    if (align === "center") {
      element.scrollTop = rowTop - element.clientHeight / 2;
    } else if (align === "start" || rowTop < element.scrollTop) {
      element.scrollTop = rowTop;
    } else if (rowTop + ROW_HEIGHT > element.scrollTop + element.clientHeight) {
      element.scrollTop = rowTop + ROW_HEIGHT - element.clientHeight;
    }
  }

  const rows = {
    count,
    top,
    bottom,
    first: Math.max(0, top - OVERSCAN),
    last: Math.min(count, bottom + 1 + OVERSCAN),
    sticky,
    groupPositions,
    scrollTo,
    onScroll: (element: HTMLDivElement) => setScrollTop(element.scrollTop),
  };
  return [viewport, rows] as const;
}

export type VirtualRows = ReturnType<typeof useVirtualRows>[1];

/** The selectable row `by` rows on from the one at position `current`,
 * staying in range; the first when nothing is selected. */
export function stepSelection<T extends { position: number }>(
  selectable: readonly T[],
  current: number,
  by: number,
): T | undefined {
  const at = selectable.findIndex((s) => s.position === current);
  const target = at === -1 ? 0 : at + by;
  return selectable[Math.min(selectable.length - 1, Math.max(0, target))];
}

interface Props {
  viewport: RefObject<HTMLDivElement | null>;
  rows: VirtualRows;
  /** Renders the row at `position`, keyed by it, with `style` placing it. */
  renderRow(position: number, style: CSSProperties): ReactNode;
  renderSticky(position: number): ReactNode;
  onKeyDown(event: KeyboardEvent<HTMLDivElement>): void;
  empty: ReactNode;
}

export default function VirtualList({
  viewport,
  rows,
  renderRow,
  renderSticky,
  onKeyDown,
  empty,
}: Props) {
  const positions = Array.from(
    { length: rows.last - rows.first },
    (_, offset) => rows.first + offset,
  );
  return (
    <div
      className="timeline"
      ref={viewport}
      tabIndex={0}
      onScroll={(event) => rows.onScroll(event.currentTarget)}
      onKeyDown={onKeyDown}
    >
      {rows.sticky !== undefined && renderSticky(rows.sticky)}
      <div
        className="timeline-content"
        style={{ height: rows.count * ROW_HEIGHT }}
      >
        {positions.map((position) =>
          renderRow(position, {
            top: position * ROW_HEIGHT,
            height: ROW_HEIGHT,
          }),
        )}
      </div>
      {rows.count === 0 && <p className="timeline-empty">{empty}</p>}
    </div>
  );
}

/** Runs the action bound to a key, if any, instead of the browser's default. */
export function handleKeys(
  event: KeyboardEvent,
  keys: Record<string, () => void>,
) {
  const action = keys[event.key];
  if (action) {
    event.preventDefault();
    action();
  }
}
