import type { ReactNode } from "react";

import Brand from "./Brand";

interface Props {
  name: string;
  meta: string;
  /** `Stat`s. */
  stats: ReactNode;
  actions: ReactNode;
  warnings: (string | null)[];
}

/** The bar above a trace or comparison: what is open, its numbers and
 * actions. */
export default function Summary({
  name,
  meta,
  stats,
  actions,
  warnings,
}: Props) {
  return (
    <header className="summary">
      <Brand />
      <div className="summary-file">
        <span className="summary-name" title={name}>
          {name}
        </span>
        <span className="summary-meta">{meta}</span>
      </div>
      <dl className="summary-stats">{stats}</dl>
      {actions}
      {warnings.map(
        (warning) =>
          warning && (
            <p key={warning} className="summary-warning">
              {warning}
            </p>
          ),
      )}
    </header>
  );
}

export function Stat({
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

export function ApproximateStat() {
  return (
    <Stat
      label="Grouping"
      value="Approximate"
      title="This protocol has no client_tick_end tick marker (added in 1.21.2), so packets are grouped into 50 ms windows"
    />
  );
}
