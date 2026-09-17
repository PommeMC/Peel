import type { Direction, Group, GroupKey, Packet, Phase } from "./trace";

export const PHASES: Phase[] = [
  "handshake",
  "status",
  "login",
  "configuration",
  "game",
];
export const DIRECTIONS: Direction[] = ["serverbound", "clientbound"];

export function directionLabel(direction: Direction): string {
  return direction === "serverbound" ? "To server" : "To client";
}

export function directionTitle(direction: Direction): string {
  return direction === "serverbound"
    ? "Serverbound: sent by the client to the server"
    : "Clientbound: sent by the server to the client";
}

export function seconds(us: number, digits = 3): string {
  return `${(us / 1e6).toFixed(digits)}s`;
}

/** A packet's time since the trace's first packet. */
export function packetTime(us: number): string {
  return `+${seconds(us)}`;
}

/** {@link packetTime} to the microsecond. */
export function preciseTime(us: number): string {
  return `+${seconds(us, 6)}`;
}

export function thousands(n: number): string {
  return n.toLocaleString("en-US");
}

export function bytes(n: number): string {
  if (n < 1000) return `${n} B`;
  if (n < 1_000_000) return `${(n / 1000).toFixed(1)} KB`;
  return `${(n / 1_000_000).toFixed(1)} MB`;
}

/** Packets without a name come from protocols this build has no tables for. */
export function packetName(packet: Packet): string {
  return packet.name ?? "unknown";
}

export function hexId(id: number): string {
  return `0x${id.toString(16).padStart(2, "0")}`;
}

export function groupLabel(group: Group): string {
  return group.kind === "window"
    ? `Window +${seconds(group.start_us)}`
    : groupKeyLabel(group);
}

export function groupKeyLabel(key: GroupKey): string {
  switch (key.kind) {
    case "setup":
      return "Before first tick";
    case "tick":
      return `Tick ${thousands(key.index ?? 0)}`;
    case "window":
      return `Window ${thousands(key.index ?? 0)}`;
  }
}

export function versionLabel(protocol: number, version: string | null): string {
  return `${version ?? "Unsupported version"} · protocol ${protocol}`;
}

/** `"1 packet"`, `"2 packets"`. */
export function count(n: number, noun: string): string {
  return `${thousands(n)} ${noun}${n === 1 ? "" : "s"}`;
}

/** Bytes in the varint at the start of `data`: a frame's packet id. */
export function varintLength(data: Uint8Array): number {
  const limit = Math.min(data.length, 5);
  for (let i = 0; i < limit; i++) {
    if ((data[i] & 0x80) === 0) return i + 1;
  }
  return limit;
}

export interface HexLine {
  start: number;
  hex: string[];
  ascii: string[];
}

/** Bytes `start` up to, not including, `end`. */
export interface ByteRange {
  start: number;
  end: number;
}

export interface HexSegment {
  text: string;
  marked: boolean;
}

/** Width of a full line's hex column: 16 bytes, space separated. */
const HEX_WIDTH = 47;
/** Where {@link hexLineText}'s hex and ASCII columns start. */
const HEX_COLUMN = 8;
const ASCII_COLUMN = HEX_COLUMN + HEX_WIDTH + 2;

/** Offset, hex and ASCII columns, 16 bytes per line. */
export function hexLines(data: Uint8Array): HexLine[] {
  const lines: HexLine[] = [];
  for (let start = 0; start < data.length; start += 16) {
    const chunk = data.subarray(start, start + 16);
    lines.push({
      start,
      hex: Array.from(chunk, hexByte),
      ascii: Array.from(chunk, (b) =>
        b >= 0x20 && b < 0x7f ? String.fromCharCode(b) : "·",
      ),
    });
  }
  return lines;
}

export function hexLineText(line: HexLine): string {
  const offset = line.start.toString(16).padStart(6, "0");
  return `${offset}  ${line.hex.join(" ").padEnd(HEX_WIDTH)}  ${line.ascii.join("")}`;
}

/** The dump's text, split so the bytes in `range` are marked in both the hex
 * and ASCII columns. */
export function hexSegments(lines: HexLine[], range: ByteRange): HexSegment[] {
  const segments: HexSegment[] = [];
  const push = (text: string, marked: boolean) => {
    if (!text) return;
    const last = segments.at(-1);
    if (last?.marked === marked) last.text += text;
    else segments.push({ text, marked });
  };
  lines.forEach((line, i) => {
    if (i > 0) push("\n", false);
    const text = hexLineText(line);
    const from = Math.max(range.start - line.start, 0);
    const to = Math.min(range.end - line.start, line.hex.length);
    if (from >= to) {
      push(text, false);
      return;
    }
    const cuts = [
      0,
      HEX_COLUMN + from * 3,
      HEX_COLUMN + to * 3 - 1,
      ASCII_COLUMN + from,
      ASCII_COLUMN + to,
      text.length,
    ];
    for (let c = 0; c + 1 < cuts.length; c++) {
      push(text.slice(cuts[c], cuts[c + 1]), c % 2 === 1);
    }
  });
  return segments;
}

/** Space-separated hex, for copying. */
export function hexText(data: Uint8Array): string {
  return Array.from(data, hexByte).join(" ");
}

function hexByte(b: number): string {
  return b.toString(16).padStart(2, "0");
}
