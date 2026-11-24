// Reads the block of bytes the backend answers a preview read with: the file's size and its
// modification time in milliseconds, both 64-bit little-endian integers, then the contents.

import type { PreviewRead } from "./types";

export const FRAME_HEADER_BYTES = 16;
/** Stands for a file whose time is not known. */
const UNKNOWN_TIME = -(2n ** 63n);

/** The reply as bytes, however the web view handed it over. */
function asBytes(reply: unknown): Uint8Array {
  if (reply instanceof ArrayBuffer) return new Uint8Array(reply);
  if (ArrayBuffer.isView(reply)) {
    return new Uint8Array(reply.buffer, reply.byteOffset, reply.byteLength);
  }
  if (Array.isArray(reply)) return Uint8Array.from(reply as number[]);
  throw new Error("The preview read returned no bytes");
}

export function parsePreviewFrame(reply: unknown): PreviewRead {
  const frame = asBytes(reply);
  if (frame.length < FRAME_HEADER_BYTES) throw new Error("The preview read was cut short");
  const header = new DataView(frame.buffer, frame.byteOffset, FRAME_HEADER_BYTES);
  const modified = header.getBigInt64(8, true);
  return {
    stamp: {
      size: Number(header.getBigUint64(0, true)),
      modified: modified === UNKNOWN_TIME ? null : Number(modified),
    },
    bytes: frame.subarray(FRAME_HEADER_BYTES),
  };
}
