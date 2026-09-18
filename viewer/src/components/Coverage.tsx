import { count, thousands } from "../lib/format";
import type { Coverage, FieldCounts } from "../lib/trace";
import { DirectionBadge } from "./Badges";

interface Props {
  coverage: Coverage;
  /** Opens the record where a packet first stopped. */
  onJump(record: number): void;
}

export function stops(counts: FieldCounts): boolean {
  return counts.partly + counts.failed > 0;
}

/** What the open trace decodes, and which packets stop part way. */
export default function Coverage({ coverage, onJump }: Props) {
  const { support, counts } = coverage;
  const stopping = coverage.packets.filter((packet) => stops(packet.counts));
  return (
    <aside className="detail">
      <h2 className="detail-name">Coverage</h2>
      <dl className="detail-fields">
        <dt>Decoded</dt>
        <dd>
          {thousands(counts.decoded)} of {thousands(counts.total)} packets
        </dd>
        <dt>Stops early</dt>
        <dd>{thousands(counts.partly + counts.failed)}</dd>
        <dt>Peel decodes</dt>
        <dd>
          {count(support.versions, "version")},{" "}
          {support.decoded === support.packets
            ? `all ${thousands(support.packets)} packets`
            : `${thousands(support.decoded)} of ${thousands(support.packets)} packets`}
        </dd>
      </dl>

      {stopping.length === 0 ? (
        <p className="detail-placeholder">
          Every packet in this trace decodes in full.
        </p>
      ) : (
        <>
          <h3 className="detail-heading">Stops early</h3>
          <ul className="finding-list">
            {stopping.map((packet) => (
              <li key={`${packet.direction}-${packet.phase}-${packet.id}`}>
                <button
                  className="finding diff-changed"
                  disabled={packet.record === undefined}
                  onClick={() =>
                    packet.record !== undefined && onJump(packet.record)
                  }
                >
                  <span className="finding-name">
                    {packet.direction === "clientbound" && (
                      <DirectionBadge direction={packet.direction} />
                    )}
                    {packet.name ?? "unknown"}
                  </span>
                  <span className="finding-meta">
                    {packet.counts.partly + packet.counts.failed} of{" "}
                    {packet.counts.total}
                  </span>
                  <span className="finding-values">{packet.reason}</span>
                </button>
              </li>
            ))}
          </ul>
        </>
      )}
    </aside>
  );
}
