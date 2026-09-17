import { useState } from "react";

import { splitTerms } from "../lib/filter";
import { count } from "../lib/format";
import type { DiffOptions } from "../lib/trace";
import { Toggle } from "./Filters";

interface Props {
  options: DiffOptions;
  onChange(update: (options: DiffOptions) => DiffOptions): void;
  onlyDifferences: boolean;
  onOnlyDifferences(only: boolean): void;
  findings: number;
}

/** What the comparison covers: directions, tick offset and ignored fields. */
export default function DiffControls({
  options,
  onChange,
  onlyDifferences,
  onOnlyDifferences,
  findings,
}: Props) {
  const [ignores, setIgnores] = useState(options.ignores.join(", "));
  return (
    <div className="filters">
      <div className="toggle-group">
        {[false, true].map((all) => (
          <Toggle
            key={String(all)}
            pressed={options.all_directions === all}
            tone={all ? "tone-all" : "direction-serverbound"}
            title={
              all
                ? "Compare everything, including the server's packets"
                : "Compare what the client sends"
            }
            onToggle={() => onChange((o) => ({ ...o, all_directions: all }))}
          >
            {all ? "All packets" : "To server"}
          </Toggle>
        ))}
      </div>
      <label
        className="control"
        title="Compares the trace's tick N + offset with the reference's tick N"
      >
        Tick offset
        <input
          className="search offset"
          type="number"
          value={options.offset}
          onChange={(event) => {
            const offset = Number(event.target.value) || 0;
            onChange((o) => ({ ...o, offset }));
          }}
        />
      </label>
      <input
        className="search"
        type="search"
        placeholder="Ignore: keep_alive, *.salt"
        title="Packets or packet.field paths to skip, separated by commas or spaces; * matches any packet"
        value={ignores}
        onChange={(event) => {
          const text = event.target.value;
          setIgnores(text);
          onChange((o) => ({ ...o, ignores: splitTerms(text) }));
        }}
      />
      <div className="toggle-group">
        <Toggle
          pressed={options.no_default_ignores}
          tone="tone-all"
          title="Also compare fields that differ between any two sessions, such as positions and ids"
          onToggle={() =>
            onChange((o) => ({
              ...o,
              no_default_ignores: !o.no_default_ignores,
            }))
          }
        >
          Session fields
        </Toggle>
        <Toggle
          pressed={onlyDifferences}
          tone="tone-all"
          onToggle={() => onOnlyDifferences(!onlyDifferences)}
        >
          Only differences
        </Toggle>
      </div>
      <span className="filters-shown">{count(findings, "finding")}</span>
    </div>
  );
}
