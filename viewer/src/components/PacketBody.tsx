import { useMemo, useState } from "react";

import {
  bytes,
  hexLines,
  hexSegments,
  hexText,
  varintLength,
} from "../lib/format";
import type { Field, LoadedTrace } from "../lib/trace";
import FieldTree from "./FieldTree";

interface Props {
  trace: LoadedTrace;
  index: number;
  /** Field paths to highlight as differing from another trace. */
  changed?: ReadonlySet<string>;
}

/** Hex dumps beyond this many bytes are cut off; chunk frames can reach megabytes. */
const DUMP_LIMIT = 64_000;

/** A record's decoded fields and frame bytes, hovering a field highlighting
 * its bytes. */
export default function PacketBody({ trace, index, changed }: Props) {
  const frame = useMemo(() => trace.frame(index), [trace, index]);
  const decoded = useMemo(() => trace.fields(index), [trace, index]);
  const lines = useMemo(
    () => (frame ? hexLines(frame.subarray(0, DUMP_LIMIT)) : []),
    [frame],
  );
  const [hovered, setHovered] = useState<Field | null>(null);
  const [copy, setCopy] = useState<"idle" | "copied" | "failed">("idle");

  const stopped =
    decoded?.status === "failed" || decoded?.status === "partial"
      ? decoded
      : null;
  const highlight = hovered
    ? { start: hovered.start, end: hovered.end, title: hovered.name }
    : stopped
      ? {
          start: stopped.at,
          end: stopped.status === "failed" ? stopped.at + 1 : Infinity,
          title: stopped.reason,
        }
      : {
          start: 0,
          end: frame ? varintLength(frame) : 0,
          title: "Packet id (varint)",
        };
  const { start, end } = highlight;
  const segments = useMemo(
    () => hexSegments(lines, { start, end }),
    [lines, start, end],
  );

  const tree = (fields: Field[]) => (
    <FieldTree fields={fields} changed={changed} onHover={setHovered} />
  );

  return (
    <>
      <h3 className="detail-heading">Fields</h3>
      {stopped ? (
        <>
          <p className="detail-note">
            {stopped.status === "failed" ? "Stopped at" : "Decoded up to"} byte{" "}
            {stopped.at}: {stopped.reason}
          </p>
          {tree(stopped.fields)}
        </>
      ) : decoded?.status !== "fields" ? (
        <p className="detail-placeholder">
          No field decoder for this packet yet. Its raw frame is below.
        </p>
      ) : decoded.fields.length === 0 ? (
        <p className="detail-placeholder">This packet has no fields.</p>
      ) : (
        tree(decoded.fields)
      )}
      <div className="detail-heading-row">
        <h3 className="detail-heading">Frame bytes</h3>
        <button
          className="button button-small"
          disabled={!frame || !navigator.clipboard}
          onClick={() => {
            if (!frame) return;
            const settle = (result: typeof copy) => {
              setCopy(result);
              setTimeout(() => setCopy("idle"), 1500);
            };
            navigator.clipboard.writeText(hexText(frame)).then(
              () => settle("copied"),
              () => settle("failed"),
            );
          }}
        >
          {{ idle: "Copy hex", copied: "Copied", failed: "Copy failed" }[copy]}
        </button>
      </div>
      {frame && frame.length > DUMP_LIMIT && (
        <p className="detail-note">
          Showing the first {bytes(DUMP_LIMIT)} of {bytes(frame.length)}.
        </p>
      )}
      <pre className="hexdump">
        {segments.map((segment, i) =>
          segment.marked ? (
            <mark key={i} title={highlight.title}>
              {segment.text}
            </mark>
          ) : (
            segment.text
          ),
        )}
      </pre>
    </>
  );
}
