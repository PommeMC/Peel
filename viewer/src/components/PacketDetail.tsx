import { useMemo, useState, type ReactNode } from "react";

import {
  bytes,
  ASCII_COLUMN,
  groupLabel,
  HEX_COLUMN,
  hexId,
  hexLines,
  hexLineText,
  hexText,
  packetName,
  preciseTime,
  thousands,
  varintLength,
  type HexLine,
} from "../lib/format";
import type { RunRow } from "../lib/rows";
import type { Group, LoadedTrace, Packet } from "../lib/trace";
import { DirectionBadge, PhaseChip } from "./Badges";

interface Props {
  trace: LoadedTrace;
  packet: Packet;
  group: Group;
  /** The run of repeats this packet belongs to, if any. */
  run?: RunRow;
}

/** Hex dumps beyond this many bytes are cut off; chunk frames can reach megabytes. */
const DUMP_LIMIT = 64_000;
const ID_TITLE = "Packet id (varint)";

export default function PacketDetail({ trace, packet, group, run }: Props) {
  const frame = useMemo(() => trace.frame(packet.index), [trace, packet.index]);
  const lines = useMemo(
    () => (frame ? hexLines(frame.subarray(0, DUMP_LIMIT)) : []),
    [frame],
  );
  const [copy, setCopy] = useState<"idle" | "copied" | "failed">("idle");
  const [head, ...rest] = lines;

  return (
    <aside className="detail">
      <h2 className={packet.name ? "detail-name" : "detail-name unknown"}>
        {packetName(packet)}
      </h2>
      <dl className="detail-fields">
        <Field label="Id">
          {hexId(packet.id)} <span className="muted">({packet.id})</span>
        </Field>
        <Field label="Direction">
          <DirectionBadge direction={packet.direction} /> {packet.direction}
        </Field>
        <Field label="Phase">
          <PhaseChip phase={packet.phase} />
        </Field>
        <Field label="Time">{preciseTime(packet.timestamp_us)}</Field>
        <Field label="Group">{groupLabel(group)}</Field>
        <Field label="Length">{bytes(packet.length)}</Field>
        <Field label="Record">#{thousands(packet.index)}</Field>
        {run && (
          <Field label="Repeats">
            {thousands(
              run.packets.findIndex((p) => p.index === packet.index) + 1,
            )}{" "}
            of {thousands(run.packets.length)} in a row
          </Field>
        )}
      </dl>
      <h3 className="detail-heading">Fields</h3>
      <p className="detail-placeholder">
        No field decoder for this packet yet. Its raw frame is below.
      </p>
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
        {head && frame && <IdLine line={head} idBytes={varintLength(frame)} />}
        {rest.length > 0 && `\n${rest.map(hexLineText).join("\n")}`}
      </pre>
    </aside>
  );
}

/** The first dump line, with the packet id's varint bytes highlighted. */
function IdLine({ line, idBytes }: { line: HexLine; idBytes: number }) {
  const text = hexLineText(line);
  const hexEnd = HEX_COLUMN + line.hex.slice(0, idBytes).join(" ").length;
  const asciiEnd = ASCII_COLUMN + idBytes;
  return (
    <>
      {text.slice(0, HEX_COLUMN)}
      <mark title={ID_TITLE}>{text.slice(HEX_COLUMN, hexEnd)}</mark>
      {text.slice(hexEnd, ASCII_COLUMN)}
      <mark title={ID_TITLE}>{text.slice(ASCII_COLUMN, asciiEnd)}</mark>
      {text.slice(asciiEnd)}
    </>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <dt>{label}</dt>
      <dd>{children}</dd>
    </>
  );
}
