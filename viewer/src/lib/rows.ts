import { packetMatcher, type Filter } from "./filter";
import type { Group, Packet, Timeline } from "./trace";

/** One line of the timeline. `ordinal` is the row's group's position in
 * `Timeline.groups`. */
export type Row =
  | {
      type: "group";
      group: Group;
      ordinal: number;
      shown: number;
      closed: boolean;
    }
  | { type: "packet"; packet: Packet; ordinal: number; inRun: boolean }
  | { type: "run"; packets: Packet[]; ordinal: number; expanded: boolean };

export type RunRow = Extract<Row, { type: "run" }>;

export interface Rows {
  rows: Row[];
  shown: number;
  /** Packets the filter shows, per group ordinal. */
  groupCounts: number[];
}

/** What the user has folded: runs they opened, by first record index, and
 * groups they closed, by ordinal. */
export interface Folds {
  openRuns: ReadonlySet<number>;
  closedGroups: ReadonlySet<number>;
}

export const NO_FOLDS: Folds = { openRuns: new Set(), closedGroups: new Set() };

function repeats(a: Packet, b: Packet): boolean {
  return a.direction === b.direction && a.phase === b.phase && a.id === b.id;
}

/** Filters the timeline into rows, folding consecutive repeats of a packet
 * into runs and leaving out the packets of closed groups. */
export function buildRows(groups: Group[], filter: Filter, folds: Folds): Rows {
  const matches = packetMatcher(filter);
  const rows: Row[] = [];
  const groupCounts: number[] = [];
  let shown = 0;
  groups.forEach((group, ordinal) => {
    const packets = group.packets.filter(matches);
    groupCounts.push(packets.length);
    if (packets.length === 0) return;
    shown += packets.length;
    const closed = folds.closedGroups.has(ordinal);
    rows.push({ type: "group", group, ordinal, shown: packets.length, closed });
    if (closed) return;
    for (let start = 0; start < packets.length;) {
      let end = start + 1;
      while (end < packets.length && repeats(packets[start], packets[end])) {
        end++;
      }
      const run = packets.slice(start, end);
      if (run.length === 1) {
        rows.push({ type: "packet", packet: run[0], ordinal, inRun: false });
      } else {
        const open = folds.openRuns.has(run[0].index);
        rows.push({ type: "run", packets: run, ordinal, expanded: open });
        if (open) {
          for (const packet of run) {
            rows.push({ type: "packet", packet, ordinal, inRun: true });
          }
        }
      }
      start = end;
    }
  });
  return { rows, shown, groupCounts };
}

export function runHolds(run: RunRow, index: number): boolean {
  return run.packets.some((p) => p.index === index);
}

/** The position of the row showing record `index`: its own row, or its run's
 * row while the run is collapsed. -1 when the filter or a closed group hides
 * it. */
export function findRecord(rows: Row[], index: number): number {
  return rows.findIndex((row) =>
    row.type === "packet"
      ? row.packet.index === index
      : row.type === "run" && !row.expanded && runHolds(row, index),
  );
}

export function runOf(rows: Row[], index: number): RunRow | undefined {
  return rows.find(
    (row): row is RunRow => row.type === "run" && runHolds(row, index),
  );
}

/** The group a "go to" entry names: a tick number, or seconds for traces
 * grouped by time. Past the end, the last group. */
export function findGroup(timeline: Timeline, value: number): number {
  const { groups } = timeline;
  const found = groups.findIndex((group) =>
    timeline.grouping === "tick"
      ? group.kind === "tick" && group.index >= value
      : group.start_us >= value * 1e6,
  );
  return found === -1 ? groups.length - 1 : found;
}
