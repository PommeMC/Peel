import { useMemo, type ReactNode } from "react";

import {
  bytes,
  groupLabel,
  hexDump,
  hexId,
  packetName,
  packetTime,
  thousands,
} from "../lib/format";
import type { Group, LoadedTrace, Packet } from "../lib/trace";
import { DirectionBadge, PhaseChip } from "./Badges";

interface Props {
  trace: LoadedTrace;
  packet: Packet;
  group: Group;
}

/** Hex dumps beyond this many bytes are cut off; chunk frames can reach megabytes. */
const DUMP_LIMIT = 64_000;

export default function PacketDetail({ trace, packet, group }: Props) {
  const frame = useMemo(() => trace.frame(packet.index), [trace, packet.index]);
  const dump = useMemo(
    () => (frame ? hexDump(frame.subarray(0, DUMP_LIMIT)) : ""),
    [frame],
  );

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
        <Field label="Time">{packetTime(packet.timestamp_us)}</Field>
        <Field label="Group">{groupLabel(group)}</Field>
        <Field label="Length">{bytes(packet.length)}</Field>
        <Field label="Record">#{thousands(packet.index)}</Field>
      </dl>
      <h3 className="detail-heading">Frame bytes</h3>
      {frame && frame.length > DUMP_LIMIT && (
        <p className="detail-note">
          Showing the first {bytes(DUMP_LIMIT)} of {bytes(frame.length)}.
        </p>
      )}
      <pre className="hexdump">{dump}</pre>
    </aside>
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
