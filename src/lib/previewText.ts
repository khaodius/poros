// Turns the bytes of a file into what the preview pane shows of them: text, decoded the way the
// editor decodes it, or rows of hexadecimal for a file that is not text.

import { withoutCutCharacter } from "./previewSniff";
import type { TextEncoding } from "./types";
import { t } from "../i18n";

/** The names the editor gives the encodings. */
export const TEXT_ENCODING_LABELS: Record<TextEncoding, string> = {
  utf8: "UTF-8",
  get utf8Bom() {
    return t("preview.utf8WithBom");
  },
  utf16Le: "UTF-16 LE",
  utf16Be: "UTF-16 BE",
  get latin1() {
    return t("preview.latin1");
  },
};

/** `String.fromCharCode` takes its characters as arguments, of which a call has a limit. */
const CHARACTERS_PER_CALL = 8192;

function latin1(bytes: Uint8Array): string {
  let text = "";
  for (let start = 0; start < bytes.length; start += CHARACTERS_PER_CALL) {
    text += String.fromCharCode(...bytes.subarray(start, start + CHARACTERS_PER_CALL));
  }
  return text;
}

function utf16(bytes: Uint8Array, littleEndian: boolean): string {
  let end = bytes.length - (bytes.length % 2);
  // The first half of a pair cut from its second half.
  if (end >= 2) {
    const last = littleEndian
      ? bytes[end - 2] | (bytes[end - 1] << 8)
      : (bytes[end - 2] << 8) | bytes[end - 1];
    if ((last & 0xfc00) === 0xd800) end -= 2;
  }
  const label = littleEndian ? "utf-16le" : "utf-16be";
  return new TextDecoder(label, { ignoreBOM: true }).decode(bytes.subarray(0, end));
}

/**
 * The text of `bytes` in the encoding the sniffing found, without its byte order mark. Unless
 * the bytes are the `complete` file, a character cut in two at their end is left out.
 */
export function decodeText(bytes: Uint8Array, encoding: TextEncoding, complete: boolean): string {
  switch (encoding) {
    case "utf8":
    case "utf8Bom": {
      const text = encoding === "utf8Bom" ? bytes.subarray(3) : bytes;
      const whole = complete ? text : withoutCutCharacter(text);
      return new TextDecoder("utf-8", { ignoreBOM: true }).decode(whole);
    }
    case "utf16Le":
      return utf16(bytes.subarray(2), true);
    case "utf16Be":
      return utf16(bytes.subarray(2), false);
    case "latin1":
      // Not the decoder's "latin1", which is Windows-1252 and changes some bytes.
      return latin1(bytes);
  }
}

/** The beginning of a longer text, up to its last whole line when it has more than one. */
export function withoutCutLine(text: string): string {
  const lastBreak = Math.max(text.lastIndexOf("\n"), text.lastIndexOf("\r"));
  return lastBreak > 0 ? text.slice(0, lastBreak + 1) : text;
}

export interface HexRow {
  /** Where the row starts in the file, in hexadecimal. */
  offset: string;
  /** The row's bytes as pairs of hexadecimal digits. */
  bytes: string[];
  /** The same bytes as characters, a dot standing for each that does not print. */
  characters: string;
}

/** Few enough bytes in a row for a row to fit a preview beside a file list. */
export const HEX_BYTES_PER_ROW = 8;

/** Offsets have at least this many digits, and only as many more as the last one needs. */
const MIN_OFFSET_DIGITS = 4;

/** The first `limit` bytes in rows, as a hex viewer shows them. */
export function hexRows(bytes: Uint8Array, limit: number, perRow = HEX_BYTES_PER_ROW): HexRow[] {
  const shown = bytes.subarray(0, limit);
  const digits = Math.max(MIN_OFFSET_DIGITS, Math.max(0, shown.length - 1).toString(16).length);
  const rows: HexRow[] = [];
  for (let start = 0; start < shown.length; start += perRow) {
    const row = [...shown.subarray(start, start + perRow)];
    rows.push({
      offset: start.toString(16).padStart(digits, "0"),
      bytes: row.map((byte) => byte.toString(16).padStart(2, "0")),
      characters: row
        .map((byte) => (byte >= 0x20 && byte < 0x7f ? String.fromCharCode(byte) : "."))
        .join(""),
    });
  }
  return rows;
}
