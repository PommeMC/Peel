import { describe, expect, it } from "vitest";

import { toggled } from "./sets";

describe("toggled", () => {
  it("adds a missing item and removes a present one without mutating", () => {
    const set = new Set([1]);
    expect([...toggled(set, 2)]).toEqual([1, 2]);
    expect([...toggled(set, 1)]).toEqual([]);
    expect([...set]).toEqual([1]);
  });
});
