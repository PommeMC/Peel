import { useRef, useState } from "react";

import Brand from "./Brand";
import FileInput from "./FileInput";

interface Props {
  onChoose(): void;
  onCompare(trace: File, reference: File): void;
  error: string | null;
  loading: string | null;
}

export default function DropZone({
  onChoose,
  onCompare,
  error,
  loading,
}: Props) {
  const [trace, setTrace] = useState<File | null>(null);
  const [reference, setReference] = useState<File | null>(null);
  const busy = loading !== null;

  return (
    <main className="landing">
      <nav className="landing-nav">
        <Brand />
      </nav>
      <section className="hero">
        <h1 className="hero-title fade-up delay-1">
          Packets,
          <br />
          <span className="accent">tick by tick.</span>
        </h1>
        <p className="hero-lede fade-up delay-2">
          Open a trace recorded with <code>peel capture</code> to see every
          packet between client and server, grouped by game tick, down to the
          byte.{" "}
          <a
            className="landing-link"
            href="https://github.com/PommeMC/Peel#capture-a-session"
          >
            How to record a trace
          </a>
        </p>
        <div className="drop-card glass fade-up delay-3">
          <span className="drop-title">Drop a .trace file anywhere</span>
          <button
            className="button button-primary"
            onClick={onChoose}
            disabled={busy}
          >
            {loading ? `Opening ${loading}…` : "Choose a trace"}
          </button>
        </div>
        <div className="drop-card glass fade-up delay-3">
          <span className="drop-title">Compare a trace against vanilla</span>
          <FileSlot label="Your trace" file={trace} onFile={setTrace} />
          <FileSlot
            label="Vanilla reference"
            file={reference}
            onFile={setReference}
          />
          <button
            className="button button-primary"
            disabled={!trace || !reference || busy}
            onClick={() => trace && reference && onCompare(trace, reference)}
          >
            Compare
          </button>
        </div>
        {error && <p className="landing-error">{error}</p>}
        <p className="landing-privacy">
          Traces are read in your browser and never uploaded.
        </p>
      </section>
    </main>
  );
}

/** A button that picks a trace and then shows its name. */
function FileSlot({
  label,
  file,
  onFile,
}: {
  label: string;
  file: File | null;
  onFile(file: File): void;
}) {
  const input = useRef<HTMLInputElement>(null);
  return (
    <>
      <FileInput input={input} onFile={onFile} />
      <button
        className="button"
        title={file?.name}
        onClick={() => input.current?.click()}
      >
        {file?.name ?? label}
      </button>
    </>
  );
}
