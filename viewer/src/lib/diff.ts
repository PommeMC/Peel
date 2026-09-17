import { count } from "./format";
import type { Diff, DiffRow, Finding, GroupDiff } from "./trace";

/** One line of the diff timeline. `ordinal` is the group's position in
 * `Diff.groups`, `index` the row's position in its group. */
export type DiffLine =
  | {
      type: "group";
      group: GroupDiff;
      ordinal: number;
      differences: number;
      open: boolean;
    }
  | { type: "row"; row: DiffRow; ordinal: number; index: number };

export type RowStatus =
  "same" | "changed" | "moved" | "shifted" | "missing" | "extra";

export const ROW_STATUS: Record<RowStatus, { marker: string; label: string }> =
  {
    same: { marker: "=", label: "The same in both" },
    changed: { marker: "≠", label: "Fields differ" },
    moved: { marker: "↕", label: "In both, in a different order" },
    shifted: { marker: "↔", label: "In both, a few ticks apart" },
    missing: { marker: "−", label: "Only in the reference" },
    extra: { marker: "+", label: "Only in the trace" },
  };

/** How far a shift moved a packet, as "3 ticks earlier". */
export function shiftLabel(shift: number): string {
  return `${count(Math.abs(shift), "tick")} ${shift < 0 ? "earlier" : "later"}`;
}

export function rowStatus(row: DiffRow): RowStatus {
  if (row.kind !== "same") return row.kind;
  return (row.fields?.length ?? 0) > 0 ? "changed" : "same";
}

export function rowDiffers(row: DiffRow): boolean {
  return rowStatus(row) !== "same";
}

/** The timeline's lines, leaving out closed groups' rows and, with
 * `onlyDifferences`, rows and groups without differences. */
export function buildDiffLines(
  diff: Diff,
  onlyDifferences: boolean,
  closed: ReadonlySet<number>,
): DiffLine[] {
  const lines: DiffLine[] = [];
  diff.groups.forEach((group, ordinal) => {
    const differences = group.rows.filter(rowDiffers).length;
    if (onlyDifferences && differences === 0) return;
    const open = !closed.has(ordinal);
    lines.push({ type: "group", group, ordinal, differences, open });
    if (!open) return;
    group.rows.forEach((row, index) => {
      if (!onlyDifferences || rowDiffers(row)) {
        lines.push({ type: "row", row, ordinal, index });
      }
    });
  });
  return lines;
}

/** Where a finding first happens, as a group ordinal and row index. */
export function findingRow(
  diff: Diff,
  finding: Finding,
): { ordinal: number; index: number } | undefined {
  const ordinal = diff.groups.findIndex(
    (group) =>
      group.key.kind === finding.group.kind &&
      group.key.index === finding.group.index,
  );
  const index =
    diff.groups[ordinal]?.rows.findIndex(
      (row) =>
        ("reference" in row ? row.reference : null) === finding.reference &&
        ("trace" in row ? row.trace : null) === finding.trace,
    ) ?? -1;
  return index === -1 ? undefined : { ordinal, index };
}

export function linePosition(
  lines: DiffLine[],
  ordinal: number,
  index: number,
): number {
  return lines.findIndex(
    (line) =>
      line.type === "row" && line.ordinal === ordinal && line.index === index,
  );
}

/** Field paths that differ in a row, for highlighting. */
export function changedPaths(row: DiffRow): ReadonlySet<string> {
  const paths = new Set<string>();
  for (const { path } of ("fields" in row ? row.fields : undefined) ?? []) {
    // Each field on the way down holds a changed one, so the tree can look a
    // row up by its own path alone.
    let prefix = "";
    for (const name of path.split(".")) {
      prefix = prefix ? `${prefix}.${name}` : name;
      paths.add(prefix);
    }
  }
  return paths;
}
