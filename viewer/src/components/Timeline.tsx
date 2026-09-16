import {
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
  type Ref,
} from "react";

import {
  bytes,
  groupLabel,
  hexId,
  packetName,
  packetTime,
  thousands,
} from "../lib/format";
import { findRecord, runHolds, runOf, type Row } from "../lib/rows";
import type { Packet } from "../lib/trace";
import { useClientSize } from "../lib/useClientSize";
import { DirectionBadge, PhaseChip } from "./Badges";

export interface TimelineHandle {
  scrollToGroup(ordinal: number): void;
  scrollToRecord(index: number): void;
}

interface Props {
  ref?: Ref<TimelineHandle>;
  rows: Row[];
  selected: number | null;
  onSelect(index: number): void;
  /** Opens or closes the run starting at record `first`. */
  onToggleRun(first: number): void;
  onToggleGroup(ordinal: number): void;
  onVisibleGroups(first: number, last: number): void;
}

const ROW_HEIGHT = 28;
/** Rows rendered beyond each edge of the viewport, so fast scrolling doesn't flash. */
const OVERSCAN = 12;

export default function Timeline({
  ref,
  rows,
  selected,
  onSelect,
  onToggleRun,
  onToggleGroup,
  onVisibleGroups,
}: Props) {
  const viewport = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const { height } = useClientSize(viewport);

  /** Rows that hold a selection: packets, and runs while collapsed. */
  const selectable = useMemo(
    () =>
      rows.flatMap((row, position) => {
        if (row.type === "packet") {
          return [{ position, index: row.packet.index }];
        }
        if (row.type === "run" && !row.expanded) {
          return [{ position, index: row.packets[0].index }];
        }
        return [];
      }),
    [rows],
  );

  const groupPositions = useMemo(
    () =>
      rows.flatMap((row, position) => (row.type === "group" ? [position] : [])),
    [rows],
  );

  function scrollTo(position: number, align: "start" | "nearest") {
    const element = viewport.current;
    if (!element) return;
    const top = position * ROW_HEIGHT;
    if (align === "start" || top < element.scrollTop) {
      element.scrollTop = top;
    } else if (top + ROW_HEIGHT > element.scrollTop + element.clientHeight) {
      element.scrollTop = top + ROW_HEIGHT - element.clientHeight;
    }
  }

  useImperativeHandle(ref, () => ({
    scrollToGroup(ordinal) {
      const position = rows.findIndex(
        (row) => row.type === "group" && row.ordinal >= ordinal,
      );
      scrollTo(
        position === -1 ? (groupPositions.at(-1) ?? 0) : position,
        "start",
      );
    },
    scrollToRecord(index) {
      const position = findRecord(rows, index);
      const element = viewport.current;
      if (position === -1 || !element) return;
      element.scrollTop = position * ROW_HEIGHT - element.clientHeight / 2;
    },
  }));

  function step(by: number) {
    if (selectable.length === 0) return;
    const position = selected === null ? -1 : findRecord(rows, selected);
    const current = selectable.findIndex((s) => s.position === position);
    const target = current === -1 ? 0 : current + by;
    const next =
      selectable[Math.min(selectable.length - 1, Math.max(0, target))];
    onSelect(next.index);
    scrollTo(next.position, "nearest");
  }

  function toggleSelectedRun(open: boolean | null) {
    if (selected === null) return;
    const run = runOf(rows, selected);
    if (run && (open === null || run.expanded !== open)) {
      onToggleRun(run.packets[0].index);
    }
  }

  const topPosition = Math.min(
    Math.floor(scrollTop / ROW_HEIGHT),
    rows.length - 1,
  );
  const bottomPosition = Math.min(
    Math.floor((scrollTop + height) / ROW_HEIGHT),
    rows.length - 1,
  );
  const topRow = rows.at(topPosition);
  const firstVisible = topRow?.ordinal;
  const lastVisible = rows.at(bottomPosition)?.ordinal;

  useEffect(() => {
    if (firstVisible !== undefined && lastVisible !== undefined) {
      onVisibleGroups(firstVisible, lastVisible);
    }
  }, [firstVisible, lastVisible, onVisibleGroups]);

  let stickyPosition: number | undefined;
  if (topRow?.type !== "group") {
    for (const position of groupPositions) {
      if (position >= topPosition) break;
      stickyPosition = position;
    }
  }
  const sticky =
    stickyPosition === undefined ? undefined : rows[stickyPosition];

  const first = Math.max(0, topPosition - OVERSCAN);
  const last = Math.min(rows.length, bottomPosition + 1 + OVERSCAN);

  return (
    <section className="timeline-panel">
      <div className="timeline-columns">
        <span>Time</span>
        <span>Direction</span>
        <span>Phase</span>
        <span>Packet</span>
        <span className="numeric">Id</span>
        <span className="numeric">Length</span>
      </div>
      <div
        className="timeline"
        ref={viewport}
        tabIndex={0}
        onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
        onKeyDown={(event) => {
          const keys: Record<string, () => void> = {
            ArrowDown: () => step(1),
            ArrowUp: () => step(-1),
            ArrowRight: () => toggleSelectedRun(true),
            ArrowLeft: () => toggleSelectedRun(false),
            Enter: () => toggleSelectedRun(null),
          };
          const action = keys[event.key];
          if (action) {
            event.preventDefault();
            action();
          }
        }}
      >
        {sticky?.type === "group" && stickyPosition !== undefined && (
          <GroupHeader
            row={sticky}
            className="group-row sticky-group"
            style={{ height: ROW_HEIGHT, marginBottom: -ROW_HEIGHT }}
            onToggle={() => {
              onToggleGroup(sticky.ordinal);
              scrollTo(stickyPosition, "start");
            }}
          />
        )}
        <div
          className="timeline-content"
          style={{ height: rows.length * ROW_HEIGHT }}
        >
          {rows.slice(first, last).map((row, offset) => {
            const position = first + offset;
            const style = { top: position * ROW_HEIGHT, height: ROW_HEIGHT };
            if (row.type === "group") {
              return (
                <GroupHeader
                  key={position}
                  row={row}
                  className="row group-row"
                  style={style}
                  onToggle={() => onToggleGroup(row.ordinal)}
                />
              );
            }
            if (row.type === "run") {
              const [head] = row.packets;
              const holdsSelection =
                !row.expanded && selected !== null && runHolds(row, selected);
              return (
                <div
                  key={position}
                  className={`row packet-row run-row${holdsSelection ? " selected" : ""}`}
                  style={style}
                  onClick={() => {
                    onToggleRun(head.index);
                    onSelect(head.index);
                  }}
                >
                  <PacketCells
                    packet={head}
                    length={row.packets.reduce((sum, p) => sum + p.length, 0)}
                    before={<Chevron open={row.expanded} />}
                    after={
                      <span className="run-count">
                        ×{thousands(row.packets.length)}
                      </span>
                    }
                  />
                </div>
              );
            }
            const { packet } = row;
            return (
              <div
                key={position}
                className={`row packet-row${row.inRun ? " in-run" : ""}${packet.index === selected ? " selected" : ""}`}
                style={style}
                onClick={() => onSelect(packet.index)}
              >
                <PacketCells packet={packet} length={packet.length} />
              </div>
            );
          })}
        </div>
        {rows.length === 0 && (
          <p className="timeline-empty">No packets match the filters.</p>
        )}
      </div>
    </section>
  );
}

function Chevron({ open }: { open: boolean }) {
  return <span className="chevron">{open ? "▾" : "▸"}</span>;
}

function GroupHeader({
  row,
  className,
  style,
  onToggle,
}: {
  row: Extract<Row, { type: "group" }>;
  className: string;
  style?: CSSProperties;
  onToggle(): void;
}) {
  return (
    <div
      className={`${className} group-${row.group.kind}`}
      style={style}
      role="button"
      aria-expanded={!row.closed}
      onClick={onToggle}
    >
      <Chevron open={!row.closed} />
      <span className="group-label">{groupLabel(row.group)}</span>
      <span className="group-meta">
        {packetTime(row.group.start_us)} · {thousands(row.shown)}{" "}
        {row.shown === 1 ? "packet" : "packets"}
      </span>
    </div>
  );
}

function PacketCells({
  packet,
  length,
  before,
  after,
}: {
  packet: Packet;
  length: number;
  before?: ReactNode;
  after?: ReactNode;
}) {
  return (
    <>
      <span className="time">{packetTime(packet.timestamp_us)}</span>
      <DirectionBadge direction={packet.direction} />
      <PhaseChip phase={packet.phase} />
      <span className={packet.name ? "name" : "name unknown"}>
        {before}
        {packetName(packet)}
        {after}
      </span>
      <span className="numeric">{hexId(packet.id)}</span>
      <span className="numeric">{bytes(length)}</span>
    </>
  );
}
