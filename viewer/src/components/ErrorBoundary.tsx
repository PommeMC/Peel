import { Component, type ErrorInfo, type ReactNode } from "react";

interface Props {
  children: ReactNode;
  /** Changing this shows the children again, e.g. when another trace opens. */
  resetKey?: unknown;
}

interface State {
  error: Error | null;
}

/** Keeps one broken view from blanking the page: a trace can come from
 * anyone, and the WebAssembly core stops rather than returning nonsense. */
export default class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidUpdate(previous: Props) {
    if (previous.resetKey !== this.props.resetKey && this.state.error) {
      this.setState({ error: null });
    }
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("Peel: view failed", error, info.componentStack);
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="detail detail-empty">
        <p>This trace couldn&apos;t be shown.</p>
        <p className="muted">{this.state.error.message}</p>
        <button
          className="button button-small"
          onClick={() => this.setState({ error: null })}
        >
          Try again
        </button>
      </div>
    );
  }
}
