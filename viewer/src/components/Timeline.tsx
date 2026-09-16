import { useEffect, useMemo, useRef, useState } from "react";

import {
  bytes,
  groupLabel,
  hexId,
  packetName,
  packetTime,
  seconds,
  thousands,
} from "../lib/format";
import type { Group, Packet } from "../lib/trace";
import { DirectionBadge, PhaseChip } from "./Badges";

export type Row =
  | { type: "group"; group: Group; shown: number }
  | { type: "packet"; packet: Packet };

interface Props {
  rows: Row[];
  selected: number | null;
  onSelect(index: number): void;
}

const ROW_HEIGHT = 28;
/** Rows rendered beyond each edge of the viewport, so fast scrolling doesn't flash. */
const OVERSCAN = 12;

export default function Timeline({ rows, selected, onSelect }: Props) {
  const viewport = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [height, setHeight] = useState(0);

  useEffect(() => {
    const element = viewport.current;
    if (!element) return;
    const observer = new ResizeObserver(() => setHeight(element.clientHeight));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const packetPositions = useMemo(
    () =>
      rows.flatMap((row, position) =>
        row.type === "packet" ? [{ position, index: row.packet.index }] : [],
      ),
    [rows],
  );

  function step(by: number) {
    if (packetPositions.length === 0) return;
    const current = packetPositions.findIndex((p) => p.index === selected);
    const target = current === -1 ? 0 : current + by;
    const next =
      packetPositions[
        Math.min(packetPositions.length - 1, Math.max(0, target))
      ];
    onSelect(next.index);
    const element = viewport.current;
    if (!element) return;
    const top = next.position * ROW_HEIGHT;
    if (top < element.scrollTop) {
      element.scrollTop = top;
    } else if (top + ROW_HEIGHT > element.scrollTop + element.clientHeight) {
      element.scrollTop = top + ROW_HEIGHT - element.clientHeight;
    }
  }

  const first = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN);
  const last = Math.min(
    rows.length,
    Math.ceil((scrollTop + height) / ROW_HEIGHT) + OVERSCAN,
  );

  return (
    <section className="timeline-panel">
      <div className="timeline-columns">
        <span>Time</span>
        <span>Dir</span>
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
          if (event.key === "ArrowDown" || event.key === "ArrowUp") {
            event.preventDefault();
            step(event.key === "ArrowDown" ? 1 : -1);
          }
        }}
      >
        <div
          className="timeline-content"
          style={{ height: rows.length * ROW_HEIGHT }}
        >
          {rows.slice(first, last).map((row, offset) => {
            const position = first + offset;
            const style = { top: position * ROW_HEIGHT, height: ROW_HEIGHT };
            if (row.type === "group") {
              return (
                <div
                  key={position}
                  className={`row group-row group-${row.group.kind}`}
                  style={style}
                >
                  <span className="group-label">{groupLabel(row.group)}</span>
                  <span className="group-meta">
                    +{seconds(row.group.start_us)} · {thousands(row.shown)}{" "}
                    {row.shown === 1 ? "packet" : "packets"}
                  </span>
                </div>
              );
            }
            const { packet } = row;
            return (
              <div
                key={position}
                className={`row packet-row${packet.index === selected ? " selected" : ""}`}
                style={style}
                onClick={() => onSelect(packet.index)}
              >
                <span className="time">{packetTime(packet.timestamp_us)}</span>
                <DirectionBadge direction={packet.direction} />
                <PhaseChip phase={packet.phase} />
                <span className={packet.name ? "name" : "name unknown"}>
                  {packetName(packet)}
                </span>
                <span className="numeric">{hexId(packet.id)}</span>
                <span className="numeric">{bytes(packet.length)}</span>
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
