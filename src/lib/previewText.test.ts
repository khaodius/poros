import { describe, expect, it } from "vitest";
import { decodeText, hexRows, withoutCutLine } from "./previewText";

const encoder = new TextEncoder();

describe("decodeText", () => {
  it("decodes UTF-8 with and without a byte order mark", () => {
    const text = "grüße ✓\n";
    expect(decodeText(encoder.encode(text), "utf8", true)).toBe(text);
    const marked = Uint8Array.from([0xef, 0xbb, 0xbf, ...encoder.encode(text)]);
    expect(decodeText(marked, "utf8Bom", true)).toBe(text);
  });

  it("leaves out a character cut in two at the end of the first part of a file", () => {
    const whole = encoder.encode("total: 5 €");
    const cut = whole.subarray(0, whole.length - 2);
    expect(decodeText(cut, "utf8", false)).toBe("total: 5 ");
    // The whole file ends there: the stray byte shows as what it is, a broken character.
    expect(decodeText(cut, "utf8", true)).toBe("total: 5 �");
  });

  it("decodes UTF-16 in either byte order", () => {
    const little = Uint8Array.from([0xff, 0xfe, 0x68, 0x00, 0x69, 0x00, 0x3d, 0xd8, 0x00, 0xde]);
    expect(decodeText(little, "utf16Le", true)).toBe("hi😀");
    const big = Uint8Array.from([0xfe, 0xff, 0x00, 0x68, 0x00, 0x69]);
    expect(decodeText(big, "utf16Be", true)).toBe("hi");
    // Cut in the middle of a unit, and between the halves of a pair.
    expect(decodeText(little.subarray(0, 5), "utf16Le", false)).toBe("h");
    expect(decodeText(little.subarray(0, 8), "utf16Le", false)).toBe("hi");
  });

  it("maps each byte to one character where the encoding is not known", () => {
    // 0x80 to 0x9f are control characters, not the punctuation Windows-1252 puts there.
    const text = decodeText(Uint8Array.from([0x63, 0x61, 0x66, 0xe9, 0x80, 0x99]), "latin1", true);
    expect(text).toBe("café\u0080\u0099");
    const long = new Uint8Array(20_000).fill(0xe9);
    expect(decodeText(long, "latin1", true)).toBe("é".repeat(20_000));
  });
});

describe("withoutCutLine", () => {
  it("ends the first part of a longer text at its last whole line", () => {
    expect(withoutCutLine("one\ntwo\nthr")).toBe("one\ntwo\n");
    expect(withoutCutLine("one\r\ntw")).toBe("one\r\n");
    expect(withoutCutLine("one\n")).toBe("one\n");
    // A single long line, as of a minified file, is kept as far as it was read.
    expect(withoutCutLine('{"a":1,"b":')).toBe('{"a":1,"b":');
  });
});

describe("hexRows", () => {
  it("lays bytes out in rows with their offsets and printable characters", () => {
    const bytes = Uint8Array.from([
      0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x7f, 0x41,
    ]);
    expect(hexRows(bytes, 256)).toEqual([
      {
        offset: "00000000",
        bytes: ["89", "50", "4e", "47", "0d", "0a", "1a", "0a"],
        characters: ".PNG....",
      },
      { offset: "00000008", bytes: ["00", "7f", "41"], characters: "..A" },
    ]);
    expect(hexRows(bytes, 256, 16)).toHaveLength(1);
  });

  it("shows no more than the limit", () => {
    const rows = hexRows(new Uint8Array(1000), 256);
    expect(rows).toHaveLength(32);
    expect(rows[31].offset).toBe("000000f8");
    expect(hexRows(new Uint8Array(0), 256)).toEqual([]);
  });
});
