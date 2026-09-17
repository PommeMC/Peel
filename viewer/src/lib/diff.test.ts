import { describe, expect, it } from "vitest";

import {
  buildDiffLines,
  changedPaths,
  findingRow,
  linePosition,
  rowDiffers,
  rowStatus,
} from "./diff";
import type { Diff, DiffRow, Finding } from "./trace";

const same: DiffRow = { kind: "same", reference: 0, trace: 0 };
const changed: DiffRow = {
  kind: "same",
  reference: 1,
  trace: 1,
  fields: [{ path: "input", reference: "0x01 forward", trace: "none" }],
};
const missing: DiffRow = { kind: "missing", reference: 2 };
const extra: DiffRow = { kind: "extra", trace: 2 };

function diff(): Diff {
  return {
    protocol: 777,
    version: "26.3",
    grouping: "tick",
    all_directions: false,
    ignores: [],
    group_counts: { compared: 2, only_in_reference: 0, only_in_trace: 1 },
    groups: [
      { key: { kind: "setup" }, reference: 0, trace: 0, rows: [same] },
      {
        key: { kind: "tick", index: 1 },
        reference: 1,
        trace: 1,
        rows: [same, changed, missing],
      },
      {
        key: { kind: "tick", index: 2 },
        reference: null,
        trace: 2,
        rows: [extra],
      },
    ],
    findings: [],
  };
}

function shape(lines: ReturnType<typeof buildDiffLines>): string[] {
  return lines.map((line) =>
    line.type === "group"
      ? `group ${line.ordinal} (${line.differences})${line.open ? "" : " closed"}`
      : `row ${line.ordinal}.${line.index}`,
  );
}

describe("buildDiffLines", () => {
  it("shows every group and row", () => {
    expect(shape(buildDiffLines(diff(), false, new Set()))).toEqual([
      "group 0 (0)",
      "row 0.0",
      "group 1 (2)",
      "row 1.0",
      "row 1.1",
      "row 1.2",
      "group 2 (1)",
      "row 2.0",
    ]);
  });

  it("keeps only differences, and closed groups' headers", () => {
    expect(shape(buildDiffLines(diff(), true, new Set([2])))).toEqual([
      "group 1 (2)",
      "row 1.1",
      "row 1.2",
      "group 2 (1) closed",
    ]);
  });
});

describe("rows", () => {
  it("differ when not the same or when fields changed", () => {
    expect([same, changed, missing, extra].map(rowStatus)).toEqual([
      "same",
      "changed",
      "missing",
      "extra",
    ]);
    expect([same, changed, missing, extra].map(rowDiffers)).toEqual([
      false,
      true,
      true,
      true,
    ]);
    expect([...changedPaths(changed)]).toEqual(["input"]);
    expect([...changedPaths(missing)]).toEqual([]);
  });

  it("locate a finding's first occurrence", () => {
    const finding = (
      group: Finding["group"],
      reference: number | null,
      trace: number | null,
    ) => ({ group, reference, trace }) as Finding;
    const d = diff();
    expect(findingRow(d, finding({ kind: "tick", index: 1 }, 2, null))).toEqual(
      {
        ordinal: 1,
        index: 2,
      },
    );
    expect(findingRow(d, finding({ kind: "tick", index: 2 }, null, 2))).toEqual(
      {
        ordinal: 2,
        index: 0,
      },
    );
    expect(findingRow(d, finding({ kind: "tick", index: 9 }, 0, 0))).toBe(
      undefined,
    );
    const lines = buildDiffLines(d, true, new Set());
    expect(linePosition(lines, 1, 2)).toBe(2);
  });
});
