import { useCallback, useEffect, useRef, useState } from "react";

import DiffView from "./components/DiffView";
import DropZone from "./components/DropZone";
import FileInput from "./components/FileInput";
import TraceView from "./components/TraceView";
import { openTrace, type LoadedTrace } from "./lib/trace";

type State =
  | { status: "empty"; error: string | null }
  | { status: "loading"; fileName: string }
  | { status: "loaded"; trace: LoadedTrace };

const EMPTY: State = { status: "empty", error: null };

/** Set when `peel view` opens the page; it serves the trace's bytes at
 * `trace`, and a reference's at `reference` when comparing. */
const served = new URLSearchParams(window.location.search);

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

function initial(path: "trace" | "reference"): State {
  const fileName = served.get(path);
  return fileName ? { status: "loading", fileName } : EMPTY;
}

export default function App() {
  const [state, setState] = useState(() => initial("trace"));
  const [reference, setReference] = useState(() => initial("reference"));
  /** Counts opened traces, so each starts with fresh view state. */
  const [opened, setOpened] = useState(0);
  const [dragging, setDragging] = useState(false);
  const traceInput = useRef<HTMLInputElement>(null);
  const referenceInput = useRef<HTMLInputElement>(null);

  const load = useCallback((set: (state: State) => void, file: File) => {
    set({ status: "loading", fileName: file.name });
    void decode(file.name, file.arrayBuffer()).then((next) => {
      set(next);
      setOpened((n) => n + 1);
    });
  }, []);

  const openFile = useCallback(
    (file: File) => {
      setReference(EMPTY);
      load(setState, file);
    },
    [load],
  );

  useEffect(() => {
    for (const [path, set] of [
      ["trace", setState],
      ["reference", setReference],
    ] as const) {
      const fileName = served.get(path);
      if (!fileName) continue;
      const data = fetch(path).then((response) => {
        if (!response.ok)
          throw new Error(`peel view answered ${response.status}`);
        return response.arrayBuffer();
      });
      void decode(fileName, data).then(set);
    }
  }, []);

  const trace = state.status === "loaded" ? state.trace : null;
  const openButton = (
    <button className="button" onClick={() => traceInput.current?.click()}>
      Open trace
    </button>
  );

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
      <FileInput input={traceInput} onFile={openFile} />
      <FileInput
        input={referenceInput}
        onFile={(file) => load(setReference, file)}
      />
      {trace && reference.status === "loaded" ? (
        <DiffView
          key={opened}
          trace={trace}
          reference={reference.trace}
          actions={
            <>
              <button
                className="button"
                onClick={() => {
                  const url = new URL(window.location.href);
                  url.searchParams.delete("reference");
                  history.replaceState(null, "", url);
                  setReference(EMPTY);
                }}
              >
                Close comparison
              </button>
              {openButton}
            </>
          }
        />
      ) : trace ? (
        <TraceView
          key={opened}
          trace={trace}
          notice={reference.status === "empty" ? reference.error : null}
          actions={
            <>
              <button
                className="button"
                disabled={reference.status === "loading"}
                title="Compare this trace against a vanilla reference trace"
                onClick={() => referenceInput.current?.click()}
              >
                {reference.status === "loading"
                  ? `Opening ${reference.fileName}…`
                  : "Compare…"}
              </button>
              {openButton}
            </>
          }
        />
      ) : (
        <DropZone
          onChoose={() => traceInput.current?.click()}
          onCompare={(file, referenceFile) => {
            load(setState, file);
            load(setReference, referenceFile);
          }}
          error={state.status === "empty" ? state.error : null}
          loading={state.status === "loading" ? state.fileName : null}
        />
      )}
      {dragging && <div className="drop-overlay">Drop to open the trace</div>}
    </div>
  );
}
