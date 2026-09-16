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
  return direction === "serverbound" ? "C→S" : "S→C";
}

export function seconds(us: number, digits = 3): string {
  return `${(us / 1e6).toFixed(digits)}s`;
}

/** A packet's time since the session started, to the microsecond. */
export function packetTime(us: number): string {
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

/** Offset, hex and ASCII columns, 16 bytes per line. */
export function hexDump(data: Uint8Array): string {
  const lines: string[] = [];
  for (let offset = 0; offset < data.length; offset += 16) {
    const chunk = data.subarray(offset, offset + 16);
    const hex = Array.from(chunk, (b) => b.toString(16).padStart(2, "0")).join(
      " ",
    );
    const ascii = Array.from(chunk, (b) =>
      b >= 0x20 && b < 0x7f ? String.fromCharCode(b) : "·",
    ).join("");
    lines.push(
      `${offset.toString(16).padStart(6, "0")}  ${hex.padEnd(47)}  ${ascii}`,
    );
  }
  return lines.join("\n");
}
