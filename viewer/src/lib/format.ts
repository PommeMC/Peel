import type { Direction, Group, Packet, Phase } from "./trace";

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
  switch (group.kind) {
    case "setup":
      return "Before first tick";
    case "tick":
      return `Tick ${thousands(group.index)}`;
    case "window":
      return `Window +${seconds(group.start_us)}`;
  }
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
  offset: string;
  hex: string[];
  ascii: string[];
}

/** Width of a full line's hex column: 16 bytes, space separated. */
const HEX_WIDTH = 47;
/** Where {@link hexLineText}'s hex and ASCII columns start. */
export const HEX_COLUMN = 8;
export const ASCII_COLUMN = HEX_COLUMN + HEX_WIDTH + 2;

/** Offset, hex and ASCII columns, 16 bytes per line. */
export function hexLines(data: Uint8Array): HexLine[] {
  const lines: HexLine[] = [];
  for (let offset = 0; offset < data.length; offset += 16) {
    const chunk = data.subarray(offset, offset + 16);
    lines.push({
      offset: offset.toString(16).padStart(6, "0"),
      hex: Array.from(chunk, hexByte),
      ascii: Array.from(chunk, (b) =>
        b >= 0x20 && b < 0x7f ? String.fromCharCode(b) : "·",
      ),
    });
  }
  return lines;
}

export function hexLineText(line: HexLine): string {
  return `${line.offset}  ${line.hex.join(" ").padEnd(HEX_WIDTH)}  ${line.ascii.join("")}`;
}

/** Space-separated hex, for copying. */
export function hexText(data: Uint8Array): string {
  return Array.from(data, hexByte).join(" ");
}

function hexByte(b: number): string {
  return b.toString(16).padStart(2, "0");
}
