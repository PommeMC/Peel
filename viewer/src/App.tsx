import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import DropZone from "./components/DropZone";
import Filters, { type Filter } from "./components/Filters";
import PacketDetail from "./components/PacketDetail";
import Summary from "./components/Summary";
import Timeline, { type Row } from "./components/Timeline";
import { packetName } from "./lib/format";
import { openTrace, type LoadedTrace, type Phase } from "./lib/trace";

type State =
  | { status: "empty"; error: string | null }
  | { status: "loading"; fileName: string }
  | { status: "loaded"; trace: LoadedTrace };

const NO_FILTER: Filter = { query: "", hiddenDirections: [], hiddenPhases: [] };

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

export default function App() {
  const [state, setState] = useState<State>(
    servedTrace
      ? { status: "loading", fileName: servedTrace }
      : { status: "empty", error: null },
  );
  const [filter, setFilter] = useState(NO_FILTER);
  const [selected, setSelected] = useState<number | null>(null);
  const [dragging, setDragging] = useState(false);
  const input = useRef<HTMLInputElement>(null);

  const show = useCallback((next: State) => {
    setFilter(NO_FILTER);
    setSelected(null);
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

  const trace = state.status === "loaded" ? state.trace : null;

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

  const { rows, shown } = useMemo(() => {
    const rows: Row[] = [];
    let shown = 0;
    const query = filter.query.trim().toLowerCase();
    for (const group of trace?.timeline.groups ?? []) {
      const matches = group.packets.filter(
        (packet) =>
          !filter.hiddenDirections.includes(packet.direction) &&
          !filter.hiddenPhases.includes(packet.phase) &&
          packetName(packet).includes(query),
      );
      if (matches.length === 0) continue;
      shown += matches.length;
      rows.push({ type: "group", group, shown: matches.length });
      for (const packet of matches) rows.push({ type: "packet", packet });
    }
    return { rows, shown };
  }, [trace, filter]);

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
            phaseCounts={phaseCounts}
            shown={shown}
            total={packets.length}
          />
          <div className="workspace">
            <Timeline rows={rows} selected={selected} onSelect={setSelected} />
            {selectedEntry ? (
              <PacketDetail
                trace={trace}
                packet={selectedEntry.packet}
                group={selectedEntry.group}
              />
            ) : (
              <aside className="detail detail-empty">
                <p>Select a packet to inspect its bytes.</p>
                <p className="muted">
                  Use ↑ and ↓ in the timeline to step through packets.
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
