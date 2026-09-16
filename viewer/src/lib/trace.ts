import init, { Trace } from "../wasm/peel_wasm";

export type Direction = "serverbound" | "clientbound";
export type Phase = "handshake" | "status" | "login" | "configuration" | "game";

/** Mirrors `peel_decode::Packet`. */
export interface Packet {
  index: number;
  timestamp_us: number;
  direction: Direction;
  phase: Phase;
  id: number;
  name: string | null;
  length: number;
}

/** Mirrors `peel_decode::Group`. */
export interface Group {
  kind: "setup" | "tick" | "window";
  index: number;
  start_us: number;
  packets: Packet[];
}

/** Mirrors `peel_decode::Timeline`, the shape of `peel dump --json`. */
export interface Timeline {
  format_version: number;
  started_at_unix_ms: number;
  recorder: string;
  protocol: number | null;
  version: string | null;
  grouping: "tick" | "approximate";
  groups: Group[];
}

export interface LoadedTrace {
  fileName: string;
  size: number;
  timeline: Timeline;
  warning: string | null;
  frame(index: number): Uint8Array | undefined;
}

let wasmReady: Promise<unknown> | undefined;

/** Decodes a trace in the browser; the bytes never leave the page. */
export async function openTrace(
  fileName: string,
  bytes: Uint8Array,
): Promise<LoadedTrace> {
  wasmReady ??= init();
  await wasmReady;
  const trace = new Trace(bytes);
  return {
    fileName,
    size: bytes.length,
    timeline: JSON.parse(trace.timeline()),
    warning: trace.warning() ?? null,
    frame: (index) => trace.frame(index),
  };
}
