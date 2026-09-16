import { bytes, seconds, thousands } from "../lib/format";
import type { LoadedTrace } from "../lib/trace";
import Brand from "./Brand";

interface Props {
  trace: LoadedTrace;
  packetCount: number;
  onOpen(): void;
}

export default function Summary({ trace, packetCount, onOpen }: Props) {
  const { timeline } = trace;
  const first = timeline.groups.at(0)?.packets.at(0)?.timestamp_us ?? 0;
  const last = timeline.groups.at(-1)?.packets.at(-1)?.timestamp_us ?? 0;
  const ticks = timeline.groups.filter((g) => g.kind === "tick").length;
  const version =
    timeline.protocol === null
      ? "No packets"
      : `${timeline.version ?? "Unsupported version"} · protocol ${timeline.protocol}`;

  return (
    <header className="summary">
      <Brand />
      <div className="summary-file">
        <span className="summary-name" title={trace.fileName}>
          {trace.fileName}
        </span>
        <span className="summary-meta">
          {version} · {new Date(timeline.started_at_unix_ms).toLocaleString()} ·{" "}
          {timeline.recorder}
        </span>
      </div>
      <dl className="summary-stats">
        <Stat label="Duration" value={seconds(last - first, 1)} />
        <Stat label="Packets" value={thousands(packetCount)} />
        {timeline.grouping === "tick" ? (
          <Stat label="Ticks" value={thousands(ticks)} />
        ) : (
          <Stat
            label="Grouping"
            value="Approximate"
            title="This protocol has no client_tick_end tick marker (added in 1.21.2), so packets are grouped into 50 ms windows"
          />
        )}
        <Stat label="Size" value={bytes(trace.size)} />
      </dl>
      <button className="button" onClick={onOpen}>
        Open trace
      </button>
      {trace.warning && <p className="summary-warning">{trace.warning}</p>}
    </header>
  );
}

function Stat({
  label,
  value,
  title,
}: {
  label: string;
  value: string;
  title?: string;
}) {
  return (
    <div className="stat" title={title}>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}
