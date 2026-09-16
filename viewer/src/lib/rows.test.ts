import { describe, expect, it } from "vitest";

import { NO_FILTER } from "./filter";
import { buildRows, findGroup, findRecord, NO_FOLDS, runOf } from "./rows";
import type { Group, Packet, Timeline } from "./trace";

let nextIndex = 0;

function packet(name: string, overrides: Partial<Packet> = {}): Packet {
  const index = nextIndex++;
  return {
    index,
    timestamp_us: index * 1000,
    direction: "clientbound",
    phase: "game",
    id: name.length,
    name,
    length: 10,
    ...overrides,
  };
}

function group(index: number, packets: Packet[]): Group {
  return {
    kind: "tick",
    index,
    start_us: packets[0]?.timestamp_us ?? 0,
    packets,
  };
}

function trace(): Group[] {
  nextIndex = 0;
  return [
    group(1, [
      packet("registry_data"),
      packet("registry_data"),
      packet("registry_data"),
      packet("keep_alive"),
    ]),
    group(2, [
      packet("registry_data"),
      packet("registry_data", { direction: "serverbound" }),
    ]),
  ];
}

const shape = (rows: ReturnType<typeof buildRows>["rows"]) =>
  rows.map((row) =>
    row.type === "group"
      ? `group ${row.ordinal}${row.closed ? " closed" : ""}`
      : row.type === "run"
        ? `run ${row.packets.map((p) => p.index).join(",")}${row.expanded ? " open" : ""}`
        : `packet ${row.packet.index}${row.inRun ? " in run" : ""}`,
  );

describe("buildRows", () => {
  it("folds consecutive repeats within a group", () => {
    const { rows, shown, groupCounts } = buildRows(
      trace(),
      NO_FILTER,
      NO_FOLDS,
    );
    expect(shape(rows)).toEqual([
      "group 0",
      "run 0,1,2",
      "packet 3",
      "group 1",
      "packet 4",
      "packet 5",
    ]);
    expect(shown).toBe(6);
    expect(groupCounts).toEqual([4, 2]);
  });

  it("lists an expanded run's packets after its row", () => {
    const { rows } = buildRows(trace(), NO_FILTER, {
      ...NO_FOLDS,
      openRuns: new Set([0]),
    });
    expect(shape(rows).slice(1, 6)).toEqual([
      "run 0,1,2 open",
      "packet 0 in run",
      "packet 1 in run",
      "packet 2 in run",
      "packet 3",
    ]);
  });

  it("forms runs after filtering and skips empty groups", () => {
    nextIndex = 0;
    const groups = [
      group(1, [
        packet("chunk_batch_start"),
        packet("level_chunk_with_light"),
        packet("set_time"),
        packet("level_chunk_with_light"),
      ]),
      group(2, [packet("keep_alive")]),
    ];
    const { rows, groupCounts } = buildRows(
      groups,
      { ...NO_FILTER, query: "level_chunk" },
      NO_FOLDS,
    );
    expect(shape(rows)).toEqual(["group 0", "run 1,3"]);
    expect(groupCounts).toEqual([2, 0]);
  });

  it("keeps a closed group's row and counts but not its packets", () => {
    const { rows, shown, groupCounts } = buildRows(trace(), NO_FILTER, {
      ...NO_FOLDS,
      closedGroups: new Set([0]),
    });
    expect(shape(rows)).toEqual([
      "group 0 closed",
      "group 1",
      "packet 4",
      "packet 5",
    ]);
    expect(shown).toBe(6);
    expect(groupCounts).toEqual([4, 2]);
    expect(findRecord(rows, 3)).toBe(-1);
  });
});

describe("findRecord and runOf", () => {
  it("finds a record's own row or its collapsed run", () => {
    const groups = trace();
    const collapsed = buildRows(groups, NO_FILTER, NO_FOLDS).rows;
    expect(findRecord(collapsed, 1)).toBe(1);
    expect(findRecord(collapsed, 3)).toBe(2);
    expect(runOf(collapsed, 2)?.packets.map((p) => p.index)).toEqual([0, 1, 2]);
    expect(runOf(collapsed, 3)).toBeUndefined();

    const expanded = buildRows(groups, NO_FILTER, {
      ...NO_FOLDS,
      openRuns: new Set([0]),
    }).rows;
    expect(findRecord(expanded, 1)).toBe(3);
    expect(findRecord(expanded, 99)).toBe(-1);
  });
});

describe("findGroup", () => {
  it("finds ticks by number and windows by time", () => {
    const groups = trace();
    const ticked: Timeline = {
      format_version: 2,
      started_at_unix_ms: 0,
      recorder: "test",
      protocol: 777,
      version: "26.3",
      grouping: "tick",
      groups,
    };
    expect(findGroup(ticked, 2)).toBe(1);
    expect(findGroup(ticked, 50)).toBe(1);
    expect(findGroup({ ...ticked, grouping: "approximate" }, 0.004)).toBe(1);
  });
});
