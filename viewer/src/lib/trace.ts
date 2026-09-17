import init, { diff, Trace } from "../wasm/peel_wasm";

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
  | {
      status: "failed" | "partial";
      fields: Field[];
      reason: string;
      at: number;
    };

/** Mirrors `peel_decode::GroupKey`: which group a packet falls in, comparable
 * across traces. */
export interface GroupKey {
  kind: Group["kind"];
  index?: number;
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
  fields(index: number): Decoded | undefined;
  /** The wasm trace, for comparing against another. */
  handle: Trace;
}

/** Mirrors `peel_decode::diff::Options`. */
export interface DiffOptions {
  all_directions: boolean;
  offset: number;
  ignores: string[];
  no_default_ignores: boolean;
}

/** Mirrors `peel_decode::diff::FieldDiff`. */
export interface FieldDiff {
  path: string;
  reference: string | null;
  trace: string | null;
}

/** Mirrors `peel_decode::diff::Row`: packets by record index in each trace. */
export type DiffRow =
  | {
      kind: "same" | "moved" | "shifted";
      reference: number;
      trace: number;
      /** For `shifted`, how many groups later the trace sends it. */
      shift?: number;
      fields?: FieldDiff[];
    }
  | { kind: "missing"; reference: number }
  | { kind: "extra"; trace: number };

/** Mirrors `peel_decode::diff::GroupDiff`. */
export interface GroupDiff {
  key: GroupKey;
  reference: number | null;
  trace: number | null;
  rows: DiffRow[];
}

/** Mirrors `peel_decode::diff::Finding`. */
export interface Finding {
  kind: "missing" | "extra" | "moved" | "shifted" | "field";
  /** For `shifted`, the first occurrence's shift in groups. */
  shift?: number;
  direction: Direction;
  phase: Phase;
  id: number;
  name: string | null;
  path?: string;
  count: number;
  compared?: number;
  group: GroupKey;
  reference: number | null;
  trace: number | null;
  reference_value?: string;
  trace_value?: string;
}

/** Mirrors `peel_decode::diff::Diff`, the shape of `peel diff --json`. */
export interface Diff {
  protocol: number;
  version: string | null;
  grouping: Timeline["grouping"];
  all_directions: boolean;
  ignores: string[];
  group_counts: {
    compared: number;
    only_in_reference: number;
    only_in_trace: number;
  };
  groups: GroupDiff[];
  findings: Finding[];
}

export interface PacketEntry {
  packet: Packet;
  group: Group;
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
    handle: trace,
  };
}

/** Compares `trace` against `reference` in the browser, as `peel diff` does. */
export function diffTraces(
  reference: LoadedTrace,
  trace: LoadedTrace,
  options: DiffOptions,
): Diff {
  return JSON.parse(
    diff(reference.handle, trace.handle, JSON.stringify(options)),
  );
}

/** Every packet with its group, by record index. */
export function indexPackets(timeline: Timeline): PacketEntry[] {
  return timeline.groups.flatMap((group) =>
    group.packets.map((packet) => ({ packet, group })),
  );
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
