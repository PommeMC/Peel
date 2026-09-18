import { describe, expect, it } from "vitest";

import { stops } from "../components/Coverage";
import type { FieldCounts } from "./trace";

function counts(partial: Partial<FieldCounts>): FieldCounts {
  return { total: 1, decoded: 0, partly: 0, failed: 0, ...partial };
}

describe("coverage rows", () => {
  it("stop when any packet decoded partly or failed", () => {
    expect(stops(counts({ decoded: 1 }))).toBe(false);
    expect(stops(counts({ partly: 1 }))).toBe(true);
    expect(stops(counts({ failed: 1 }))).toBe(true);
  });
});
