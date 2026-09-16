import { describe, expect, it } from "vitest";

import {
  ASCII_COLUMN,
  HEX_COLUMN,
  hexLines,
  hexLineText,
  hexText,
  packetTime,
  preciseTime,
  varintLength,
} from "./format";

describe("varintLength", () => {
  it("counts bytes up to the first without a continuation bit", () => {
    expect(varintLength(new Uint8Array([0x2e, 0xff]))).toBe(1);
    expect(varintLength(new Uint8Array([0xac, 0x02, 0x00]))).toBe(2);
    expect(varintLength(new Uint8Array([0x80, 0x80, 0x80, 0x80, 0x01]))).toBe(
      5,
    );
  });

  it("stops at five bytes or the end of the data", () => {
    expect(varintLength(new Uint8Array([0x80, 0x80, 0x80, 0x80, 0x80]))).toBe(
      5,
    );
    expect(varintLength(new Uint8Array([0x80]))).toBe(1);
    expect(varintLength(new Uint8Array([]))).toBe(0);
  });
});

describe("hex dump", () => {
  const data = new Uint8Array(20).map((_, i) => 0x41 + i);

  it("splits into 16-byte lines with a partial last line", () => {
    const lines = hexLines(data);
    expect(lines).toHaveLength(2);
    expect(lines[1]).toEqual({
      offset: "000010",
      hex: ["51", "52", "53", "54"],
      ascii: ["Q", "R", "S", "T"],
    });
    expect(hexLineText(lines[1])).toBe(
      `000010  ${"51 52 53 54".padEnd(47)}  QRST`,
    );
  });

  it("places the hex and ASCII columns where the id highlight expects", () => {
    const [line] = hexLines(data);
    const text = hexLineText(line);
    expect(text.slice(HEX_COLUMN, HEX_COLUMN + 5)).toBe("41 42");
    expect(text.slice(ASCII_COLUMN, ASCII_COLUMN + 2)).toBe("AB");
  });

  it("shows non-printable bytes as dots and copies plain hex", () => {
    const bytes = new Uint8Array([0x00, 0x7f, 0x20]);
    expect(hexLines(bytes)[0].ascii).toEqual(["·", "·", " "]);
    expect(hexText(bytes)).toBe("00 7f 20");
  });
});

describe("times", () => {
  it("formats milliseconds and microseconds", () => {
    expect(packetTime(1_234_567)).toBe("+1.235s");
    expect(preciseTime(1_234_567)).toBe("+1.234567s");
  });
});
