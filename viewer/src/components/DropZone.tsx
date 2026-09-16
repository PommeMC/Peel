import Brand from "./Brand";

interface Props {
  onChoose(): void;
  error: string | null;
  loading: string | null;
}

export default function DropZone({ onChoose, error, loading }: Props) {
  return (
    <main className="landing">
      <div className="landing-card">
        <Brand large />
        <p className="landing-tagline">Minecraft packets, tick by tick.</p>
        <button
          className="drop-target"
          onClick={onChoose}
          disabled={loading !== null}
        >
          {loading ? (
            <span className="drop-title">Opening {loading}…</span>
          ) : (
            <>
              <span className="drop-title">Drop a .trace file here</span>
              <span className="drop-hint">or click to choose one</span>
            </>
          )}
        </button>
        {error && <p className="landing-error">{error}</p>}
        <p className="landing-privacy">
          Traces are read in your browser and never uploaded. Record one with{" "}
          <code>peel capture</code>.
        </p>
      </div>
    </main>
  );
}
