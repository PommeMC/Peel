import {
  useEffect,
  useImperativeHandle,
  useMemo,
  type CSSProperties,
  type ReactNode,
  type Ref,
} from "react";

import {
  bytes,
  count,
  groupLabel,
  hexId,
  packetName,
  packetTime,
  thousands,
} from "../lib/format";
import { findRecord, runHolds, runOf, type Row } from "../lib/rows";
import type { Packet } from "../lib/trace";
import { DirectionBadge, PhaseChip } from "./Badges";
import GroupHeader, { Chevron } from "./GroupHeader";
import VirtualList, {
  handleKeys,
  stepSelection,
  useVirtualRows,
} from "./VirtualList";

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

export default function Timeline({
  ref,
  rows,
  selected,
  onSelect,
  onToggleRun,
  onToggleGroup,
  onVisibleGroups,
}: Props) {
  const [viewport, list] = useVirtualRows(rows);

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

  useImperativeHandle(ref, () => ({
    scrollToGroup(ordinal) {
      const position = rows.findIndex(
        (row) => row.type === "group" && row.ordinal >= ordinal,
      );
      list.scrollTo(
        position === -1 ? (list.groupPositions.at(-1) ?? 0) : position,
        "start",
      );
    },
    scrollToRecord(index) {
      const position = findRecord(rows, index);
      if (position !== -1) list.scrollTo(position, "center");
    },
  }));

  function step(by: number) {
    const current = selected === null ? -1 : findRecord(rows, selected);
    const next = stepSelection(selectable, current, by);
    if (!next) return;
    onSelect(next.index);
    list.scrollTo(next.position, "nearest");
  }

  function toggleSelectedRun(open: boolean | null) {
    if (selected === null) return;
    const run = runOf(rows, selected);
    if (run && (open === null || run.expanded !== open)) {
      onToggleRun(run.packets[0].index);
    }
  }

  const firstVisible = rows.at(list.top)?.ordinal;
  const lastVisible = rows.at(list.bottom)?.ordinal;

  useEffect(() => {
    if (firstVisible !== undefined && lastVisible !== undefined) {
      onVisibleGroups(firstVisible, lastVisible);
    }
  }, [firstVisible, lastVisible, onVisibleGroups]);

  /** A group's header, placed by `style` or pinned when it has none. */
  function header(position: number, style?: CSSProperties) {
    const row = rows[position];
    if (row.type !== "group") return null;
    return (
      <GroupHeader
        key={position}
        label={groupLabel(row.group)}
        meta={`${packetTime(row.group.start_us)} · ${count(row.shown, "packet")}`}
        open={!row.closed}
        tone={row.group.kind}
        rows={list}
        position={position}
        style={style}
        onToggle={() => onToggleGroup(row.ordinal)}
      />
    );
  }

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
      <VirtualList
        viewport={viewport}
        rows={list}
        empty="No packets match the filters."
        onKeyDown={(event) =>
          handleKeys(event, {
            ArrowDown: () => step(1),
            ArrowUp: () => step(-1),
            ArrowRight: () => toggleSelectedRun(true),
            ArrowLeft: () => toggleSelectedRun(false),
            Enter: () => toggleSelectedRun(null),
          })
        }
        renderSticky={(position) => header(position)}
        renderRow={(position, style) => {
          const row = rows[position];
          if (row.type === "group") return header(position, style);
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
        }}
      />
    </section>
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
