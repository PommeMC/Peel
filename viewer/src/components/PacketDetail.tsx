import { useMemo, useState, type ReactNode } from "react";

import {
  bytes,
  groupLabel,
  hexId,
  hexLines,
  hexSegments,
  hexText,
  packetName,
  preciseTime,
  thousands,
  varintLength,
} from "../lib/format";
import type { RunRow } from "../lib/rows";
import type { Decoded, Field, Group, LoadedTrace, Packet } from "../lib/trace";
import { DirectionBadge, PhaseChip } from "./Badges";
import FieldTree, { type FieldTreeProps } from "./FieldTree";

interface Props {
  trace: LoadedTrace;
  packet: Packet;
  group: Group;
  /** The run of repeats this packet belongs to, if any. */
  run?: RunRow;
}

/** Hex dumps beyond this many bytes are cut off; chunk frames can reach megabytes. */
const DUMP_LIMIT = 64_000;

export default function PacketDetail({ trace, packet, group, run }: Props) {
  const frame = useMemo(() => trace.frame(packet.index), [trace, packet.index]);
  const lines = useMemo(
    () => (frame ? hexLines(frame.subarray(0, DUMP_LIMIT)) : []),
    [frame],
  );
  const decoded = useMemo(
    () => trace.fields(packet.index),
    [trace, packet.index],
  );
  const [hovered, setHovered] = useState<Field | null>(null);
  const [copy, setCopy] = useState<"idle" | "copied" | "failed">("idle");

  const highlight = hovered
    ? { start: hovered.start, end: hovered.end, title: hovered.name }
    : decoded?.status === "failed"
      ? { start: decoded.at, end: decoded.at + 1, title: decoded.error }
      : {
          start: 0,
          end: frame ? varintLength(frame) : 0,
          title: "Packet id (varint)",
        };
  const { start, end } = highlight;
  const segments = useMemo(
    () => hexSegments(lines, { start, end }),
    [lines, start, end],
  );

  return (
    <aside className="detail">
      <h2 className={packet.name ? "detail-name" : "detail-name unknown"}>
        {packetName(packet)}
      </h2>
      <dl className="detail-fields">
        <Detail label="Id">
          {hexId(packet.id)} <span className="muted">({packet.id})</span>
        </Detail>
        <Detail label="Direction">
          <DirectionBadge direction={packet.direction} /> {packet.direction}
        </Detail>
        <Detail label="Phase">
          <PhaseChip phase={packet.phase} />
        </Detail>
        <Detail label="Time">{preciseTime(packet.timestamp_us)}</Detail>
        <Detail label="Group">{groupLabel(group)}</Detail>
        <Detail label="Length">{bytes(packet.length)}</Detail>
        <Detail label="Record">#{thousands(packet.index)}</Detail>
        {run && (
          <Detail label="Repeats">
            {thousands(
              run.packets.findIndex((p) => p.index === packet.index) + 1,
            )}{" "}
            of {thousands(run.packets.length)} in a row
          </Detail>
        )}
      </dl>
      <h3 className="detail-heading">Fields</h3>
      <Fields decoded={decoded} onHover={setHovered} />
      <div className="detail-heading-row">
        <h3 className="detail-heading">Frame bytes</h3>
        <button
          className="button button-small"
          disabled={!frame || !navigator.clipboard}
          onClick={() => {
            if (!frame) return;
            const settle = (result: typeof copy) => {
              setCopy(result);
              setTimeout(() => setCopy("idle"), 1500);
            };
            navigator.clipboard.writeText(hexText(frame)).then(
              () => settle("copied"),
              () => settle("failed"),
            );
          }}
        >
          {{ idle: "Copy hex", copied: "Copied", failed: "Copy failed" }[copy]}
        </button>
      </div>
      {frame && frame.length > DUMP_LIMIT && (
        <p className="detail-note">
          Showing the first {bytes(DUMP_LIMIT)} of {bytes(frame.length)}.
        </p>
      )}
      <pre className="hexdump">
        {segments.map((segment, i) =>
          segment.marked ? (
            <mark key={i} title={highlight.title}>
              {segment.text}
            </mark>
          ) : (
            segment.text
          ),
        )}
      </pre>
    </aside>
  );
}

function Fields({
  decoded,
  onHover,
}: Pick<FieldTreeProps, "onHover"> & { decoded: Decoded | undefined }) {
  if (decoded?.status === "failed") {
    return (
      <>
        <p className="detail-note">
          Stopped at byte {decoded.at}: {decoded.error}
        </p>
        <FieldTree fields={decoded.fields} onHover={onHover} />
      </>
    );
  }
  if (decoded?.status !== "fields") {
    return (
      <p className="detail-placeholder">
        No field decoder for this packet yet. Its raw frame is below.
      </p>
    );
  }
  if (decoded.fields.length === 0) {
    return <p className="detail-placeholder">This packet has no fields.</p>;
  }
  return <FieldTree fields={decoded.fields} onHover={onHover} />;
}

function Detail({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <dt>{label}</dt>
      <dd>{children}</dd>
    </>
  );
}
