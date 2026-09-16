import { directionLabel } from "../lib/format";
import type { Direction, Phase } from "../lib/trace";

export function DirectionBadge({ direction }: { direction: Direction }) {
  return (
    <span className={`badge direction-${direction}`}>
      {directionLabel(direction)}
    </span>
  );
}

export function PhaseChip({ phase }: { phase: Phase }) {
  return <span className={`chip phase-${phase}`}>{phase}</span>;
}
