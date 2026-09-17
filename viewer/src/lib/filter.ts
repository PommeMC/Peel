import { packetName } from "./format";
import type { Direction, Packet, Phase } from "./trace";

export interface Filter {
  query: string;
  hiddenDirections: ReadonlySet<Direction>;
  hiddenPhases: ReadonlySet<Phase>;
}

export const NO_FILTER: Filter = {
  query: "",
  hiddenDirections: new Set(),
  hiddenPhases: new Set(),
};

export interface Query {
  include: string[];
  exclude: string[];
}

/** The non-empty terms of a list separated by commas or spaces. */
export function splitTerms(text: string): string[] {
  return text.split(/[\s,]+/).filter(Boolean);
}

/** Terms are separated by commas or spaces; a leading `-` excludes names
 * containing the rest. */
export function parseQuery(query: string): Query {
  const include: string[] = [];
  const exclude: string[] = [];
  for (const term of splitTerms(query.toLowerCase())) {
    if (term.startsWith("-")) {
      if (term.length > 1) exclude.push(term.slice(1));
    } else {
      include.push(term);
    }
  }
  return { include, exclude };
}

/** A predicate for packets the filter shows, parsing its query once. */
export function packetMatcher(filter: Filter): (packet: Packet) => boolean {
  const { include, exclude } = parseQuery(filter.query);
  return (packet) => {
    if (
      filter.hiddenDirections.has(packet.direction) ||
      filter.hiddenPhases.has(packet.phase)
    ) {
      return false;
    }
    const name = packetName(packet);
    return (
      (include.length === 0 || include.some((term) => name.includes(term))) &&
      !exclude.some((term) => name.includes(term))
    );
  };
}
