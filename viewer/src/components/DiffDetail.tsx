import { changedPaths, ROW_STATUS, rowStatus } from "../lib/diff";
import { groupLabel, preciseTime, thousands } from "../lib/format";
import type { DiffRow, LoadedTrace, PacketEntry } from "../lib/trace";
import PacketBody from "./PacketBody";
import { Detail, PacketName } from "./PacketDetail";

interface Props {
  reference: LoadedTrace;
  trace: LoadedTrace;
  row: DiffRow;
  /** Each trace's packets by record index: reference, then trace. */
  packets: [PacketEntry[], PacketEntry[]];
}

/** Both sides of a diff row, differing fields highlighted. */
export default function DiffDetail({ reference, trace, row, packets }: Props) {
  const status = rowStatus(row);
  const changed = changedPaths(row);
  const sides = [
    "reference" in row && {
      label: "Reference",
      trace: reference,
      entry: packets[0][row.reference],
    },
    "trace" in row && { label: "Trace", trace, entry: packets[1][row.trace] },
  ];
  return (
    <aside className="detail">
      <p className={`diff-status diff-${status}`}>{ROW_STATUS[status].label}</p>
      {sides.map(
        (side) =>
          side &&
          side.entry && (
            <section key={side.label} className="diff-side">
              <h3 className="detail-heading">
                {side.label} · {side.trace.fileName}
              </h3>
              <PacketName packet={side.entry.packet} />
              <dl className="detail-fields">
                <Detail label="Time">
                  {preciseTime(side.entry.packet.timestamp_us)}
                </Detail>
                <Detail label="Group">{groupLabel(side.entry.group)}</Detail>
                <Detail label="Record">
                  #{thousands(side.entry.packet.index)}
                </Detail>
              </dl>
              <PacketBody
                trace={side.trace}
                index={side.entry.packet.index}
                changed={changed}
              />
            </section>
          ),
      )}
    </aside>
  );
}
