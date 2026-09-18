import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";

import { NO_FILTER } from "../lib/filter";
import { bytes, seconds, thousands, versionLabel } from "../lib/format";
import { buildRows, findGroup, NO_FOLDS, runOf, type Folds } from "../lib/rows";
import { toggled } from "../lib/sets";
import { usePendingScroll } from "../lib/usePendingScroll";
import { indexPackets, type LoadedTrace, type Phase } from "../lib/trace";
import Activity from "./Activity";
import Coverage from "./Coverage";
import Filters from "./Filters";
import PacketDetail, { EmptyDetail } from "./PacketDetail";
import Summary, { ApproximateStat, Stat } from "./Summary";
import Timeline, { type TimelineHandle } from "./Timeline";

interface Props {
  trace: LoadedTrace;
  actions: ReactNode;
  notice: string | null;
}

/** The record a `#record=N` link points at, if the trace has it. */
function linkedRecord(trace: LoadedTrace): number | null {
  const match = /^#record=(\d+)$/.exec(window.location.hash);
  if (!match) return null;
  const record = Number(match[1]);
  const total = trace.timeline.groups.reduce((n, g) => n + g.packets.length, 0);
  return record < total ? record : null;
}

/** One trace's timeline, filters and packet detail. */
export default function TraceView({ trace, actions, notice }: Props) {
  const [filter, setFilter] = useState(NO_FILTER);
  const [selected, setSelected] = useState(() => linkedRecord(trace));
  const [folds, setFolds] = useState<Folds>(() => {
    const run =
      selected === null
        ? undefined
        : runOf(
            buildRows(trace.timeline.groups, NO_FILTER, NO_FOLDS).rows,
            selected,
          );
    return {
      ...NO_FOLDS,
      openRuns: new Set(run ? [run.packets[0].index] : []),
    };
  });
  const [visible, setVisible] = useState<[number, number] | null>(null);
  const [showCoverage, setShowCoverage] = useState(false);
  const search = useRef<HTMLInputElement>(null);
  const timeline = useRef<TimelineHandle>(null);
  usePendingScroll(selected, (record) =>
    timeline.current?.scrollToRecord(record),
  );

  useEffect(() => {
    const focusSearch = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement;
      if (
        event.key === "/" &&
        !["INPUT", "TEXTAREA"].includes(target.tagName)
      ) {
        event.preventDefault();
        search.current?.focus();
      }
    };
    window.addEventListener("keydown", focusSearch);
    return () => window.removeEventListener("keydown", focusSearch);
  }, []);

  useEffect(() => {
    const hash = selected === null ? "" : `#record=${selected}`;
    const { pathname, search } = window.location;
    history.replaceState(null, "", `${pathname}${search}${hash}`);
  }, [selected]);

  const packets = useMemo(() => indexPackets(trace.timeline), [trace]);
  // Decoding every packet takes a moment, so only when the panel is opened.
  const coverage = useMemo(
    () => (showCoverage ? trace.coverage() : null),
    [trace, showCoverage],
  );

  const phaseCounts = useMemo(() => {
    const counts = new Map<Phase, number>();
    for (const { packet } of packets)
      counts.set(packet.phase, (counts.get(packet.phase) ?? 0) + 1);
    return counts;
  }, [packets]);

  const { rows, shown, groupCounts } = useMemo(
    () => buildRows(trace.timeline.groups, filter, folds),
    [trace, filter, folds],
  );

  const toggleRun = useCallback((first: number) => {
    setFolds((f) => ({ ...f, openRuns: toggled(f.openRuns, first) }));
  }, []);

  const toggleGroup = useCallback((ordinal: number) => {
    setFolds((f) => ({ ...f, closedGroups: toggled(f.closedGroups, ordinal) }));
  }, []);

  const showVisible = useCallback((first: number, last: number) => {
    setVisible((current) =>
      current?.[0] === first && current[1] === last ? current : [first, last],
    );
  }, []);

  const { timeline: data } = trace;
  const firstTime = data.groups.at(0)?.packets.at(0)?.timestamp_us ?? 0;
  const lastTime = data.groups.at(-1)?.packets.at(-1)?.timestamp_us ?? 0;
  const ticks = data.groups.filter((g) => g.kind === "tick").length;
  const selectedEntry = selected === null ? undefined : packets[selected];

  return (
    <>
      <Summary
        name={trace.fileName}
        meta={[
          data.protocol === null
            ? "No packets"
            : versionLabel(data.protocol, data.version),
          new Date(data.started_at_unix_ms).toLocaleString(),
          data.recorder,
        ].join(" · ")}
        stats={
          <>
            <Stat label="Duration" value={seconds(lastTime - firstTime, 1)} />
            <Stat label="Packets" value={thousands(packets.length)} />
            {data.grouping === "tick" ? (
              <Stat label="Ticks" value={thousands(ticks)} />
            ) : (
              <ApproximateStat />
            )}
            <Stat label="Size" value={bytes(trace.size)} />
          </>
        }
        actions={
          <>
            <button
              className="button"
              title="What this trace decodes, and where decoding stops"
              onClick={() => setShowCoverage((shown) => !shown)}
            >
              {showCoverage ? "Close coverage" : "Coverage"}
            </button>
            {actions}
          </>
        }
        warnings={[trace.warning, notice]}
      />
      <Filters
        filter={filter}
        onChange={setFilter}
        searchRef={search}
        grouping={data.grouping}
        onJump={(value) =>
          timeline.current?.scrollToGroup(findGroup(data, value))
        }
        phaseCounts={phaseCounts}
        shown={shown}
        total={packets.length}
      />
      <Activity
        groups={data.groups}
        counts={groupCounts}
        visible={visible}
        onJump={(ordinal) => timeline.current?.scrollToGroup(ordinal)}
      />
      <div className="workspace">
        <Timeline
          ref={timeline}
          rows={rows}
          selected={selected}
          onSelect={setSelected}
          onToggleRun={toggleRun}
          onToggleGroup={toggleGroup}
          onVisibleGroups={showVisible}
        />
        {coverage ? (
          <Coverage
            coverage={coverage}
            onJump={(record) => {
              setSelected(record);
              setShowCoverage(false);
            }}
          />
        ) : selectedEntry ? (
          <PacketDetail
            key={selected}
            trace={trace}
            packet={selectedEntry.packet}
            group={selectedEntry.group}
            run={runOf(rows, selectedEntry.packet.index)}
          />
        ) : (
          <EmptyDetail
            title="Select a packet to inspect its bytes."
            hint="Use ↑ and ↓ in the timeline to step through packets, and → or ← to open or close repeats."
          />
        )}
      </div>
    </>
  );
}
