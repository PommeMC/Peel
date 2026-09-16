import Brand from "./Brand";

interface Props {
  onChoose(): void;
  error: string | null;
  loading: string | null;
}

export default function DropZone({ onChoose, error, loading }: Props) {
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
          byte.
        </p>
        <div className="drop-card glass fade-up delay-3">
          <span className="drop-title">Drop a .trace file anywhere</span>
          <button
            className="button button-primary"
            onClick={onChoose}
            disabled={loading !== null}
          >
            {loading ? `Opening ${loading}…` : "Choose a trace"}
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
