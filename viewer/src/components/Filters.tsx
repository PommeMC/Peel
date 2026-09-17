import type { ReactNode, Ref } from "react";

import type { Filter } from "../lib/filter";
import {
  count,
  DIRECTIONS,
  PHASES,
  directionLabel,
  directionTitle,
  thousands,
} from "../lib/format";
import { toggled } from "../lib/sets";
import type { Phase, Timeline } from "../lib/trace";

interface Props {
  filter: Filter;
  /** Takes an update rather than a value, so quick successive clicks compose. */
  onChange(update: (filter: Filter) => Filter): void;
  searchRef?: Ref<HTMLInputElement>;
  grouping: Timeline["grouping"];
  /** A tick number, or seconds when the trace is grouped by time. */
  onJump(value: number): void;
  phaseCounts: Map<Phase, number>;
  shown: number;
  total: number;
}

/** A pill that is pressed while its items are shown. */
export function Toggle({
  pressed,
  tone,
  title,
  onToggle,
  children,
}: {
  pressed: boolean;
  tone: string;
  title?: string;
  onToggle(): void;
  children: ReactNode;
}) {
  return (
    <button
      className={`toggle ${tone}`}
      aria-pressed={pressed}
      title={title}
      onClick={onToggle}
    >
      {children}
    </button>
  );
}

export default function Filters({
  filter,
  onChange,
  searchRef,
  grouping,
  onJump,
  phaseCounts,
  shown,
  total,
}: Props) {
  return (
    <div className="filters">
      <input
        ref={searchRef}
        className="search"
        type="search"
        placeholder="Filter: name, -exclude"
        title="Comma- or space-separated names; prefix a name with - to hide it. Press / to focus."
        value={filter.query}
        onChange={(event) => {
          const query = event.target.value;
          onChange((f) => ({ ...f, query }));
        }}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            onChange((f) => ({ ...f, query: "" }));
            event.currentTarget.blur();
          }
        }}
      />
      <form
        onSubmit={(event) => {
          event.preventDefault();
          const value = String(new FormData(event.currentTarget).get("jump"));
          if (value.trim() !== "") onJump(Number(value));
        }}
      >
        <input
          className="search jump"
          name="jump"
          type="number"
          min={0}
          step={grouping === "tick" ? 1 : "any"}
          placeholder={grouping === "tick" ? "Go to tick" : "Go to time (s)"}
        />
      </form>
      <div className="toggle-group">
        {DIRECTIONS.map((direction) => (
          <Toggle
            key={direction}
            pressed={!filter.hiddenDirections.has(direction)}
            tone={`direction-${direction}`}
            title={directionTitle(direction)}
            onToggle={() =>
              onChange((f) => ({
                ...f,
                hiddenDirections: toggled(f.hiddenDirections, direction),
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
            pressed={!filter.hiddenPhases.has(phase)}
            tone={`phase-${phase}`}
            onToggle={() =>
              onChange((f) => ({
                ...f,
                hiddenPhases: toggled(f.hiddenPhases, phase),
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
          ? count(total, "packet")
          : `${thousands(shown)} of ${count(total, "packet")}`}
      </span>
    </div>
  );
}
