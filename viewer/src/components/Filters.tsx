import type { ReactNode } from "react";

import { DIRECTIONS, PHASES, directionLabel, thousands } from "../lib/format";
import type { Direction, Phase } from "../lib/trace";

export interface Filter {
  query: string;
  hiddenDirections: Direction[];
  hiddenPhases: Phase[];
}

interface Props {
  filter: Filter;
  /** Takes an update rather than a value, so quick successive clicks compose. */
  onChange(update: (filter: Filter) => Filter): void;
  phaseCounts: Map<Phase, number>;
  shown: number;
  total: number;
}

function toggle<T>(list: T[], item: T): T[] {
  return list.includes(item) ? list.filter((i) => i !== item) : [...list, item];
}

/** A pill that is pressed while its items are shown. */
function Toggle({
  pressed,
  tone,
  onToggle,
  children,
}: {
  pressed: boolean;
  tone: string;
  onToggle(): void;
  children: ReactNode;
}) {
  return (
    <button
      className={`toggle ${tone}`}
      aria-pressed={pressed}
      onClick={onToggle}
    >
      {children}
    </button>
  );
}

export default function Filters({
  filter,
  onChange,
  phaseCounts,
  shown,
  total,
}: Props) {
  return (
    <div className="filters">
      <input
        className="search"
        type="search"
        placeholder="Filter by packet name"
        value={filter.query}
        onChange={(event) => {
          const query = event.target.value;
          onChange((f) => ({ ...f, query }));
        }}
      />
      <div className="toggle-group">
        {DIRECTIONS.map((direction) => (
          <Toggle
            key={direction}
            pressed={!filter.hiddenDirections.includes(direction)}
            tone={`direction-${direction}`}
            onToggle={() =>
              onChange((f) => ({
                ...f,
                hiddenDirections: toggle(f.hiddenDirections, direction),
              }))
            }
          >
            {directionLabel(direction)}
          </Toggle>
        ))}
      </div>
      <div className="toggle-group">
        {PHASES.filter((phase) => phaseCounts.has(phase)).map((phase) => (
          <Toggle
            key={phase}
            pressed={!filter.hiddenPhases.includes(phase)}
            tone={`phase-${phase}`}
            onToggle={() =>
              onChange((f) => ({
                ...f,
                hiddenPhases: toggle(f.hiddenPhases, phase),
              }))
            }
          >
            {phase}
            <span className="toggle-count">
              {thousands(phaseCounts.get(phase) ?? 0)}
            </span>
          </Toggle>
        ))}
      </div>
      <span className="filters-shown">
        {shown === total
          ? `${thousands(total)} packets`
          : `${thousands(shown)} of ${thousands(total)} packets`}
      </span>
    </div>
  );
}
