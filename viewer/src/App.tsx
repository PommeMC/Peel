import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import Activity from "./components/Activity";
import DropZone from "./components/DropZone";
import Filters from "./components/Filters";
import PacketDetail from "./components/PacketDetail";
import Summary from "./components/Summary";
import Timeline, { type TimelineHandle } from "./components/Timeline";
import { NO_FILTER } from "./lib/filter";
import { buildRows, findGroup, NO_FOLDS, runOf, type Folds } from "./lib/rows";
import { toggled } from "./lib/sets";
import { openTrace, type LoadedTrace, type Phase } from "./lib/trace";

type State =
  | { status: "empty"; error: string | null }
  | { status: "loading"; fileName: string }
  | { status: "loaded"; trace: LoadedTrace };

/** Set when `peel view` opens the page; it serves the trace's bytes at `trace`. */
const servedTrace = new URLSearchParams(window.location.search).get("trace");

async function decode(
  fileName: string,
  data: Promise<ArrayBuffer>,
): Promise<State> {
  try {
    const trace = await openTrace(fileName, new Uint8Array(await data));
    return { status: "loaded", trace };
  } catch (err) {
    const reason = err instanceof Error ? err.message : String(err);
    return { status: "empty", error: `Couldn't open ${fileName}: ${reason}` };
  }
}

/** The record a `#record=N` link points at, if the trace has it. */
function linkedRecord(trace: LoadedTrace): number | null {
  const match = /^#record=(\d+)$/.exec(window.location.hash);
  if (!match) return null;
  const record = Number(match[1]);
  const total = trace.timeline.groups.reduce((n, g) => n + g.packets.length, 0);
  return record < total ? record : null;
}

export default function App() {
  const [state, setState] = useState<State>(
    servedTrace
      ? { status: "loading", fileName: servedTrace }
      : { status: "empty", error: null },
  );
  const [filter, setFilter] = useState(NO_FILTER);
  const [selected, setSelected] = useState<number | null>(null);
  const [folds, setFolds] = useState<Folds>(NO_FOLDS);
  const [visible, setVisible] = useState<[number, number] | null>(null);
  const [dragging, setDragging] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const search = useRef<HTMLInputElement>(null);
  const timeline = useRef<TimelineHandle>(null);
  /** A record to scroll to once the rows showing it have rendered. */
  const pendingScroll = useRef<number | null>(null);

  const show = useCallback((next: State) => {
    const record = next.status === "loaded" ? linkedRecord(next.trace) : null;
    const run =
      next.status === "loaded" && record !== null
        ? runOf(
            buildRows(next.trace.timeline.groups, NO_FILTER, NO_FOLDS).rows,
            record,
          )
        : undefined;
    setFilter(NO_FILTER);
    setSelected(record);
    setFolds({
      ...NO_FOLDS,
      openRuns: new Set(run ? [run.packets[0].index] : []),
    });
    setVisible(null);
    pendingScroll.current = record;
    setState(next);
  }, []);

  const openFile = useCallback(
    (file: File) => {
      setState({ status: "loading", fileName: file.name });
      void decode(file.name, file.arrayBuffer()).then(show);
    },
    [show],
  );

  useEffect(() => {
    if (!servedTrace) return;
    const data = fetch("trace").then((response) => {
      if (!response.ok)
        throw new Error(`peel view answered ${response.status}`);
      return response.arrayBuffer();
    });
    void decode(servedTrace, data).then(show);
  }, [show]);

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

  const trace = state.status === "loaded" ? state.trace : null;

  useEffect(() => {
    if (!trace) return;
    const hash = selected === null ? "" : `#record=${selected}`;
    const { pathname, search } = window.location;
    history.replaceState(null, "", `${pathname}${search}${hash}`);
  }, [trace, selected]);

  const packets = useMemo(
    () =>
      trace?.timeline.groups.flatMap((group) =>
        group.packets.map((packet) => ({ packet, group })),
      ) ?? [],
    [trace],
  );

  const phaseCounts = useMemo(() => {
    const counts = new Map<Phase, number>();
    for (const { packet } of packets)
      counts.set(packet.phase, (counts.get(packet.phase) ?? 0) + 1);
    return counts;
  }, [packets]);

  const { rows, shown, groupCounts } = useMemo(
    () => buildRows(trace?.timeline.groups ?? [], filter, folds),
    [trace, filter, folds],
  );

  useEffect(() => {
    if (pendingScroll.current === null) return;
    timeline.current?.scrollToRecord(pendingScroll.current);
    pendingScroll.current = null;
  }, [rows]);

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

  const selectedEntry = selected === null ? undefined : packets[selected];

  return (
    <div
      className="app"
      onDragOver={(event) => {
        event.preventDefault();
        setDragging(true);
      }}
      onDragLeave={(event) => {
        if (event.relatedTarget === null) setDragging(false);
      }}
      onDrop={(event) => {
        event.preventDefault();
        setDragging(false);
        const file = event.dataTransfer.files[0];
        if (file) openFile(file);
      }}
    >
      <input
        ref={input}
        type="file"
        accept=".trace"
        hidden
        onChange={(event) => {
          const file = event.target.files?.[0];
          if (file) openFile(file);
          event.target.value = "";
        }}
      />
      {trace ? (
        <>
          <Summary
            trace={trace}
            packetCount={packets.length}
            onOpen={() => input.current?.click()}
          />
          <Filters
            filter={filter}
            onChange={setFilter}
            searchRef={search}
            grouping={trace.timeline.grouping}
            onJump={(value) =>
              timeline.current?.scrollToGroup(findGroup(trace.timeline, value))
            }
            phaseCounts={phaseCounts}
            shown={shown}
            total={packets.length}
          />
          <Activity
            groups={trace.timeline.groups}
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
            {selectedEntry ? (
              <PacketDetail
                key={selected}
                trace={trace}
                packet={selectedEntry.packet}
                group={selectedEntry.group}
                run={runOf(rows, selectedEntry.packet.index)}
              />
            ) : (
              <aside className="detail detail-empty">
                <p>Select a packet to inspect its bytes.</p>
                <p className="muted">
                  Use ↑ and ↓ in the timeline to step through packets, and → or
                  ← to open or close repeats.
                </p>
              </aside>
            )}
          </div>
        </>
      ) : (
        <DropZone
          onChoose={() => input.current?.click()}
          error={state.status === "empty" ? state.error : null}
          loading={state.status === "loading" ? state.fileName : null}
        />
      )}
      {dragging && <div className="drop-overlay">Drop to open the trace</div>}
    </div>
  );
}
