import { describe, expect, it } from "vitest";

import { NO_FILTER, packetMatcher, parseQuery } from "./filter";
import type { Packet } from "./trace";

function packet(name: string | null, overrides: Partial<Packet> = {}): Packet {
  return {
    index: 0,
    timestamp_us: 0,
    direction: "clientbound",
    phase: "game",
    id: 0,
    name,
    length: 1,
    ...overrides,
  };
}

describe("parseQuery", () => {
  it("splits on commas and spaces and lowercases", () => {
    expect(parseQuery(" Move_Player,player_input  keep ")).toEqual({
      include: ["move_player", "player_input", "keep"],
      exclude: [],
    });
  });

  it("treats a leading dash as an exclusion", () => {
    expect(parseQuery("chunk, -light -")).toEqual({
      include: ["chunk"],
      exclude: ["light"],
    });
  });

  it("is empty for a blank query", () => {
    expect(parseQuery("  , ")).toEqual({ include: [], exclude: [] });
  });
});

describe("packetMatcher", () => {
  const matches = (query: string, name: string | null) =>
    packetMatcher({ ...NO_FILTER, query })(packet(name));

  it("matches any include term", () => {
    expect(matches("move_player, player_input", "player_input")).toBe(true);
    expect(matches("move_player, player_input", "keep_alive")).toBe(false);
  });

  it("drops names containing an exclude term", () => {
    expect(matches("-rotate_head", "rotate_head")).toBe(false);
    expect(matches("-rotate_head", "keep_alive")).toBe(true);
    expect(matches("entity, -rotate", "set_entity_motion")).toBe(true);
    expect(matches("entity, -motion", "set_entity_motion")).toBe(false);
  });

  it("matches unnamed packets as unknown", () => {
    expect(matches("unknown", null)).toBe(true);
  });

  it("respects the direction and phase toggles", () => {
    const hidden = packetMatcher({
      ...NO_FILTER,
      hiddenDirections: new Set(["clientbound"]),
    });
    expect(hidden(packet("keep_alive"))).toBe(false);
    const phase = packetMatcher({
      ...NO_FILTER,
      hiddenPhases: new Set(["game"]),
    });
    expect(phase(packet("keep_alive"))).toBe(false);
  });
});
