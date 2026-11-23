import { describe, expect, it } from "vitest";
import { FRAME_HEADER_BYTES, parsePreviewFrame } from "./previewFrame";

/** A frame as the backend builds it. */
function frame(size: bigint, modified: bigint, contents: number[]): ArrayBuffer {
  const buffer = new ArrayBuffer(FRAME_HEADER_BYTES + contents.length);
  const view = new DataView(buffer);
  view.setBigUint64(0, size, true);
  view.setBigInt64(8, modified, true);
  new Uint8Array(buffer, FRAME_HEADER_BYTES).set(contents);
  return buffer;
}

describe("parsePreviewFrame", () => {
  it("reads the file's size and time, then the contents", () => {
    const read = parsePreviewFrame(frame(5_000_000_000n, 1_700_000_000_123n, [0x89, 0x50, 0x4e]));
    expect(read.stamp).toEqual({ size: 5_000_000_000, modified: 1_700_000_000_123 });
    expect([...read.bytes]).toEqual([0x89, 0x50, 0x4e]);
  });

  it("reads a time that is not known, and one before 1970", () => {
    expect(parsePreviewFrame(frame(0n, -(2n ** 63n), [])).stamp).toEqual({
      size: 0,
      modified: null,
    });
    expect(parsePreviewFrame(frame(1n, -86_400_000n, [7])).stamp.modified).toBe(-86_400_000);
  });

  it("takes the bytes however the web view hands them over", () => {
    const buffer = frame(3n, 9n, [1, 2, 3]);
    const expected = { stamp: { size: 3, modified: 9 }, bytes: Uint8Array.from([1, 2, 3]) };
    expect(parsePreviewFrame(new Uint8Array(buffer))).toEqual(expected);
    expect(parsePreviewFrame([...new Uint8Array(buffer)])).toEqual(expected);
    // A view into a larger buffer.
    const padded = new Uint8Array(buffer.byteLength + 8);
    padded.set(new Uint8Array(buffer), 4);
    expect(parsePreviewFrame(padded.subarray(4, 4 + buffer.byteLength))).toEqual(expected);
  });

  it("refuses a reply that is no frame", () => {
    expect(() => parsePreviewFrame(new ArrayBuffer(FRAME_HEADER_BYTES - 1))).toThrow();
    expect(() => parsePreviewFrame(null)).toThrow();
    expect(() => parsePreviewFrame("bytes")).toThrow();
  });
});
