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

/** Mirrors `peel_decode::fields::Field`. */
export interface Field {
  name: string;
  value?: string;
  start: number;
  end: number;
  children?: Field[];
}

/** Mirrors `peel_decode::fields::Decoded`. */
export type Decoded =
  | { status: "fields"; fields: Field[] }
  | { status: "no_decoder" }
  | { status: "failed"; fields: Field[]; error: string; at: number };

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
  fields(index: number): Decoded | undefined;
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
  const timeline: Timeline = JSON.parse(trace.timeline());
  startAtFirstPacket(timeline);
  return {
    fileName,
    size: bytes.length,
    timeline,
    warning: trace.warning() ?? null,
    frame: (index) => trace.frame(index),
    fields: (index) => {
      const json = trace.fields(index);
      return json === undefined ? undefined : JSON.parse(json);
    },
  };
}

/** Shifts times so the first packet is at zero; older traces count from when
 * capture started rather than from the recorded connection. */
function startAtFirstPacket(timeline: Timeline) {
  const origin = timeline.groups.at(0)?.packets.at(0)?.timestamp_us ?? 0;
  for (const group of timeline.groups) {
    group.start_us = Math.max(0, group.start_us - origin);
    for (const packet of group.packets) packet.timestamp_us -= origin;
  }
}
