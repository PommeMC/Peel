import { useCallback, useMemo, useRef, useState, type ReactNode } from "react";

import { buildDiffLines, findingRow } from "../lib/diff";
import { count, thousands, versionLabel } from "../lib/format";
import { toggled } from "../lib/sets";
import { usePendingScroll } from "../lib/usePendingScroll";
import {
  diffTraces,
  indexPackets,
  type Diff,
  type DiffOptions,
  type Finding,
  type LoadedTrace,
  type PacketEntry,
} from "../lib/trace";
import DiffControls from "./DiffControls";
import DiffDetail from "./DiffDetail";
import DiffTimeline, {
  type DiffTimelineHandle,
  type RowAddress,
} from "./DiffTimeline";
import Findings from "./Findings";
import { EmptyDetail } from "./PacketDetail";
import Summary, { ApproximateStat, Stat } from "./Summary";

interface Props {
  trace: LoadedTrace;
  reference: LoadedTrace;
  actions: ReactNode;
}

const DEFAULT_OPTIONS: DiffOptions = {
  all_directions: false,
  offset: 0,
  ignores: [],
  no_default_ignores: false,
};

/** A trace compared against a vanilla reference. */
export default function DiffView({ trace, reference, actions }: Props) {
  const [options, setOptions] = useState(DEFAULT_OPTIONS);
  const [onlyDifferences, setOnlyDifferences] = useState(true);
  const [closed, setClosed] = useState<ReadonlySet<number>>(new Set());
  const [selected, setSelected] = useState<RowAddress | null>(null);
  const timeline = useRef<DiffTimelineHandle>(null);
  const scrollWhenRendered = usePendingScroll<RowAddress>(null, (row) =>
    timeline.current?.scrollToRow(row),
  );

  const result = useMemo((): { diff: Diff } | { error: string } => {
    try {
      return { diff: diffTraces(reference, trace, options) };
    } catch (err) {
      return { error: err instanceof Error ? err.message : String(err) };
    }
  }, [reference, trace, options]);
  const diff = "diff" in result ? result.diff : null;

  const lines = useMemo(
    () => (diff ? buildDiffLines(diff, onlyDifferences, closed) : []),
    [diff, onlyDifferences, closed],
  );
  const packets = useMemo(
    (): [PacketEntry[], PacketEntry[]] => [
      indexPackets(reference.timeline),
      indexPackets(trace.timeline),
    ],
    [reference, trace],
  );

  /** Group ordinals depend on the options, so changing them starts over. */
  const changeOptions = useCallback(
    (update: (options: DiffOptions) => DiffOptions) => {
      setOptions(update);
      setClosed(new Set());
      setSelected(null);
    },
    [],
  );

  function jump(finding: Finding) {
    const row = diff && findingRow(diff, finding);
    if (!row) return;
    setClosed((c) => (c.has(row.ordinal) ? toggled(c, row.ordinal) : c));
    setSelected(row);
    scrollWhenRendered(row);
  }

  const selectedRow =
    selected && diff?.groups[selected.ordinal]?.rows[selected.index];

  return (
    <>
      <Summary
        name={`${trace.fileName} against ${reference.fileName}`}
        meta={
          diff
            ? `${versionLabel(diff.protocol, diff.version)} · ${count(diff.group_counts.only_in_reference, "group")} only in the reference, ${count(diff.group_counts.only_in_trace, "group")} only in the trace`
            : "Can't compare these traces"
        }
        stats={
          <>
            <Stat
              label="Compared"
              value={thousands(diff?.group_counts.compared ?? 0)}
            />
            <Stat
              label="Findings"
              value={thousands(diff?.findings.length ?? 0)}
            />
            {diff?.grouping === "approximate" && <ApproximateStat />}
          </>
        }
        actions={actions}
        warnings={[
          "error" in result ? result.error : null,
          reference.warning,
          trace.warning,
        ]}
      />
      <DiffControls
        options={options}
        onChange={changeOptions}
        onlyDifferences={onlyDifferences}
        onOnlyDifferences={setOnlyDifferences}
        findings={diff?.findings.length ?? 0}
      />
      <div className="workspace">
        <Findings findings={diff?.findings ?? []} onJump={jump} />
        <DiffTimeline
          ref={timeline}
          lines={lines}
          packets={packets}
          selected={selected}
          onSelect={({ ordinal, index }) => setSelected({ ordinal, index })}
          onToggleGroup={(ordinal) => setClosed((c) => toggled(c, ordinal))}
        />
        {selectedRow ? (
          <DiffDetail
            key={`${selected.ordinal}.${selected.index}`}
            reference={reference}
            trace={trace}
            row={selectedRow}
            packets={packets}
          />
        ) : (
          <EmptyDetail
            title="Select a row to compare its packets."
            hint="Findings on the left jump to where each difference first happens."
          />
        )}
      </div>
    </>
  );
}
