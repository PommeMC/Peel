import { useRef, type PointerEvent } from "react";

import { count, groupLabel } from "../lib/format";
import type { Group } from "../lib/trace";
import { useClientSize } from "../lib/useClientSize";

interface Props {
  groups: Group[];
  /** Packets the filters show, per group. */
  counts: number[];
  /** Ordinals of the first and last groups on screen. */
  visible: [number, number] | null;
  onJump(ordinal: number): void;
}

const HEIGHT = 44;
/** Narrowest bar, in pixels; longer traces bucket several groups per bar. */
const BAR_WIDTH = 3;

export default function Activity({ groups, counts, visible, onJump }: Props) {
  const container = useRef<HTMLDivElement>(null);
  const { width } = useClientSize(container);

  const buckets = Math.max(
    1,
    Math.min(counts.length, Math.floor(width / BAR_WIDTH)),
  );
  const perBucket = Math.ceil(counts.length / buckets);
  const bars = Array.from(
    { length: Math.ceil(counts.length / perBucket) },
    (_, i) =>
      counts
        .slice(i * perBucket, (i + 1) * perBucket)
        .reduce((sum, n) => sum + n, 0),
  );
  const tallest = Math.max(1, ...bars);
  const bucketOf = (ordinal: number) => Math.floor(ordinal / perBucket);
  const barWidth = width / Math.max(1, bars.length);

  function jump(event: PointerEvent<SVGSVGElement>) {
    const x = event.clientX - event.currentTarget.getBoundingClientRect().left;
    const bar = Math.min(
      bars.length - 1,
      Math.max(0, Math.floor(x / barWidth)),
    );
    onJump(bar * perBucket);
  }

  return (
    <div className="activity" ref={container}>
      <svg
        width={width}
        height={HEIGHT}
        onPointerDown={(event) => {
          event.currentTarget.setPointerCapture(event.pointerId);
          jump(event);
        }}
        onPointerMove={(event) => {
          if (event.buttons === 1) jump(event);
        }}
      >
        {visible && (
          <rect
            className="activity-visible"
            x={bucketOf(visible[0]) * barWidth}
            width={(bucketOf(visible[1]) - bucketOf(visible[0]) + 1) * barWidth}
            height={HEIGHT}
          />
        )}
        {bars.map((packets, i) => {
          const barHeight = (packets / tallest) * (HEIGHT - 6);
          const firstGroup = groups[i * perBucket];
          const lastGroup =
            groups[Math.min(groups.length, (i + 1) * perBucket) - 1];
          const range =
            firstGroup === lastGroup
              ? groupLabel(firstGroup)
              : `${groupLabel(firstGroup)} – ${groupLabel(lastGroup)}`;
          return (
            <rect
              key={i}
              className="activity-bar"
              x={i * barWidth}
              y={HEIGHT - barHeight}
              width={Math.max(1, barWidth - 1)}
              height={barHeight}
            >
              <title>{`${range} · ${count(packets, "packet")}`}</title>
            </rect>
          );
        })}
      </svg>
    </div>
  );
}
