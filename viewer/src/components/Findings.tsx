import { count, groupKeyLabel } from "../lib/format";
import type { Finding } from "../lib/trace";
import { DirectionBadge } from "./Badges";

const KINDS: { kind: Finding["kind"]; heading: string; tone: string }[] = [
  { kind: "missing", heading: "Missing from the trace", tone: "diff-missing" },
  { kind: "extra", heading: "Extra in the trace", tone: "diff-extra" },
  { kind: "moved", heading: "In a different order", tone: "diff-moved" },
  { kind: "field", heading: "Fields that differ", tone: "diff-changed" },
];

interface Props {
  findings: Finding[];
  onJump(finding: Finding): void;
}

/** Differences grouped by packet and field; each opens its first occurrence. */
export default function Findings({ findings, onJump }: Props) {
  return (
    <aside className="findings">
      {findings.length === 0 && (
        <p className="findings-empty">No differences.</p>
      )}
      {KINDS.map(({ kind, heading, tone }) => {
        const items = findings.filter((f) => f.kind === kind);
        if (items.length === 0) return null;
        return (
          <section key={kind}>
            <h3 className="detail-heading">{heading}</h3>
            <ul>
              {items.map((finding, i) => (
                <li key={i}>
                  <button
                    className={`finding ${tone}`}
                    onClick={() => onJump(finding)}
                  >
                    <span className="finding-name">
                      {finding.direction === "clientbound" && (
                        <DirectionBadge direction={finding.direction} />
                      )}
                      {finding.name ?? "unknown"}
                      {finding.path && `.${finding.path}`}
                    </span>
                    <span className="finding-meta">
                      {finding.compared === undefined
                        ? count(finding.count, "packet")
                        : `${count(finding.count, "difference")} in ${count(finding.compared, "pair")}`}
                      {" · first in "}
                      {groupKeyLabel(finding.group)}
                    </span>
                    {finding.kind === "field" && (
                      <span className="finding-values">
                        {finding.reference_value ?? "(none)"} →{" "}
                        {finding.trace_value ?? "(none)"}
                      </span>
                    )}
                  </button>
                </li>
              ))}
            </ul>
          </section>
        );
      })}
    </aside>
  );
}
