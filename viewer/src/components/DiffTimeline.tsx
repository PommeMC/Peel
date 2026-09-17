import {
  useImperativeHandle,
  useMemo,
  type CSSProperties,
  type Ref,
} from "react";

import {
  linePosition,
  ROW_STATUS,
  rowStatus,
  type DiffLine,
} from "../lib/diff";
import { count, groupKeyLabel, packetName, packetTime } from "../lib/format";
import type { Packet, PacketEntry } from "../lib/trace";
import GroupHeader from "./GroupHeader";
import VirtualList, {
  handleKeys,
  stepSelection,
  useVirtualRows,
} from "./VirtualList";

export interface RowAddress {
  ordinal: number;
  index: number;
}

export interface DiffTimelineHandle {
  scrollToRow(row: RowAddress): void;
}

interface Props {
  ref?: Ref<DiffTimelineHandle>;
  lines: DiffLine[];
  /** Each trace's packets by record index: reference, then trace. */
  packets: [PacketEntry[], PacketEntry[]];
  selected: RowAddress | null;
  onSelect(row: RowAddress): void;
  onToggleGroup(ordinal: number): void;
}

/** The reference and trace side by side, row by aligned row. */
export default function DiffTimeline({
  ref,
  lines,
  packets,
  selected,
  onSelect,
  onToggleGroup,
}: Props) {
  const [viewport, list] = useVirtualRows(lines);
  const selectable = useMemo(
    () =>
      lines.flatMap((line, position) =>
        line.type === "row"
          ? [{ position, ordinal: line.ordinal, index: line.index }]
          : [],
      ),
    [lines],
  );

  useImperativeHandle(ref, () => ({
    scrollToRow({ ordinal, index }) {
      const position = linePosition(lines, ordinal, index);
      if (position !== -1) list.scrollTo(position, "center");
    },
  }));

  function step(by: number) {
    const current = selected
      ? linePosition(lines, selected.ordinal, selected.index)
      : -1;
    const next = stepSelection(selectable, current, by);
    if (!next) return;
    onSelect(next);
    list.scrollTo(next.position, "nearest");
  }

  /** A group's header, placed by `style` or pinned when it has none. */
  function header(position: number, style?: CSSProperties) {
    const line = lines[position];
    if (line.type !== "group") return null;
    return (
      <GroupHeader
        key={position}
        label={groupKeyLabel(line.group.key)}
        meta={count(line.differences, "difference")}
        open={line.open}
        tone={line.group.key.kind}
        rows={list}
        position={position}
        style={style}
        onToggle={() => onToggleGroup(line.ordinal)}
      />
    );
  }

  return (
    <section className="timeline-panel">
      <div className="timeline-columns diff-row">
        <span>Time</span>
        <span>Reference</span>
        <span />
        <span>Time</span>
        <span>Trace</span>
      </div>
      <VirtualList
        viewport={viewport}
        rows={list}
        empty="No differences."
        onKeyDown={(event) =>
          handleKeys(event, {
            ArrowDown: () => step(1),
            ArrowUp: () => step(-1),
          })
        }
        renderSticky={(position) => header(position)}
        renderRow={(position, style) => {
          const line = lines[position];
          if (line.type === "group") return header(position, style);
          const { row } = line;
          const status = rowStatus(row);
          const isSelected =
            selected?.ordinal === line.ordinal && selected.index === line.index;
          return (
            <div
              key={position}
              className={`row packet-row diff-row diff-${status}${isSelected ? " selected" : ""}`}
              style={style}
              onClick={() => onSelect(line)}
            >
              <PacketCells
                packet={
                  "reference" in row
                    ? packets[0][row.reference]?.packet
                    : undefined
                }
              />
              <span className="diff-marker" title={ROW_STATUS[status].label}>
                {ROW_STATUS[status].marker}
              </span>
              <PacketCells
                packet={
                  "trace" in row ? packets[1][row.trace]?.packet : undefined
                }
              />
            </div>
          );
        }}
      />
    </section>
  );
}

function PacketCells({ packet }: { packet: Packet | undefined }) {
  if (!packet)
    return (
      <>
        <span />
        <span />
      </>
    );
  return (
    <>
      <span className="time">{packetTime(packet.timestamp_us)}</span>
      <span className={packet.name ? "name" : "name unknown"}>
        {packetName(packet)}
      </span>
    </>
  );
}
