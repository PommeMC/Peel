import type { RefObject } from "react";

/** A hidden `.trace` file picker, opened by clicking `input`. */
export default function FileInput({
  input,
  onFile,
}: {
  input: RefObject<HTMLInputElement | null>;
  onFile(file: File): void;
}) {
  return (
    <input
      ref={input}
      type="file"
      accept=".trace"
      hidden
      onChange={(event) => {
        const file = event.target.files?.[0];
        if (file) onFile(file);
        event.target.value = "";
      }}
    />
  );
}
