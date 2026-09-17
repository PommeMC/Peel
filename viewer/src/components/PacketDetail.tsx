import type { ReactNode } from "react";

import {
  bytes,
  groupLabel,
  hexId,
  packetName,
  preciseTime,
  thousands,
} from "../lib/format";
import type { RunRow } from "../lib/rows";
import type { Group, LoadedTrace, Packet } from "../lib/trace";
import { DirectionBadge, PhaseChip } from "./Badges";
import PacketBody from "./PacketBody";

interface Props {
  trace: LoadedTrace;
  packet: Packet;
  group: Group;
  /** The run of repeats this packet belongs to, if any. */
  run?: RunRow;
}

export default function PacketDetail({ trace, packet, group, run }: Props) {
  return (
    <aside className="detail">
      <PacketName packet={packet} />
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
      <PacketBody trace={trace} index={packet.index} />
    </aside>
  );
}

/** The detail panel before anything is selected. */
export function EmptyDetail({ title, hint }: { title: string; hint: string }) {
  return (
    <aside className="detail detail-empty">
      <p>{title}</p>
      <p className="muted">{hint}</p>
    </aside>
  );
}

export function PacketName({ packet }: { packet: Packet }) {
  return (
    <h2 className={packet.name ? "detail-name" : "detail-name unknown"}>
      {packetName(packet)}
    </h2>
  );
}

export function Detail({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <>
      <dt>{label}</dt>
      <dd>{children}</dd>
    </>
  );
}
