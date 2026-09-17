import type { CSSProperties } from "react";

import { ROW_HEIGHT, type VirtualRows } from "./VirtualList";

export function Chevron({ open }: { open: boolean }) {
  return <span className="chevron">{open ? "▾" : "▸"}</span>;
}

interface Props {
  label: string;
  meta: string;
  open: boolean;
  /** Tints the label, e.g. `window` for approximate groups. */
  tone: string;
  rows: VirtualRows;
  position: number;
  /** Places the header as a list row; without it the header is pinned over
   * the rows it heads, taking no space, and toggling it scrolls back to its
   * row. */
  style?: CSSProperties;
  onToggle(): void;
}

/** A timeline group's title bar, which opens or closes the group. */
export default function GroupHeader({
  label,
  meta,
  open,
  tone,
  rows,
  position,
  style,
  onToggle,
}: Props) {
  return (
    <div
      className={`group-row group-${tone} ${style ? "row" : "sticky-group"}`}
      style={style ?? { height: ROW_HEIGHT, marginBottom: -ROW_HEIGHT }}
      role="button"
      aria-expanded={open}
      onClick={() => {
        onToggle();
        if (!style) rows.scrollTo(position, "start");
      }}
    >
      <Chevron open={open} />
      <span className="group-label">{label}</span>
      <span className="group-meta">{meta}</span>
    </div>
  );
}
