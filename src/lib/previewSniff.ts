// Tells what a file is from its first bytes, with its name as a second opinion, for the preview
// pane. A name alone decides nothing: a picture is one by its signature, text by its bytes.

import type { TextEncoding } from "./types";

/** The pictures the web view decodes. */
export type ImageFormat = "png" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "avif" | "svg";

export const IMAGE_MIME_TYPES: Record<ImageFormat, string> = {
  png: "image/png",
  jpeg: "image/jpeg",
  gif: "image/gif",
  webp: "image/webp",
  bmp: "image/bmp",
  ico: "image/x-icon",
  avif: "image/avif",
  svg: "image/svg+xml",
};

export const IMAGE_FORMAT_LABELS: Record<ImageFormat, string> = {
  png: "PNG",
  jpeg: "JPEG",
  gif: "GIF",
  webp: "WebP",
  bmp: "BMP",
  ico: "ICO",
  avif: "AVIF",
  svg: "SVG",
};

export type Sniffed =
  | { kind: "empty" }
  | { kind: "image"; format: ImageFormat }
  | { kind: "text"; encoding: TextEncoding }
  /** `description` names what the bytes are where their signature tells, as "ZIP archive". */
  | { kind: "binary"; description: string | null };

/** What a file's name suggests, which decides only how much of the file is read first. */
export type NameHint = "image" | "opaque" | "unknown";

const IMAGE_EXTENSIONS = new Set("png jpg jpeg jpe jfif gif webp bmp ico avif svg".split(" "));

/** Files that are never text and cannot be shown: a look at their signature is enough. */
const OPAQUE_EXTENSIONS = new Set(
  (
    "zip tar gz tgz bz2 tbz2 xz txz zst 7z rar iso img dmg deb rpm jar war apk cab " +
    "mp4 mkv mov avi webm m4v wmv flv mpg mpeg mp3 flac wav ogg oga ogv m4a aac opus wma aiff " +
    "pdf doc docx xls xlsx ppt pptx odt ods odp epub " +
    "tif tiff heic heif psd cr2 nef dng xcf svgz " +
    "exe dll so dylib bin o a class pyc wasm msi " +
    "ttf otf woff woff2 sqlite sqlite3 db parquet"
  ).split(" "),
);

/** How far into a file the editor looks for the NUL bytes that mark it as binary. */
const BINARY_SNIFF_BYTES = 8000;
/** As much of a text as is read to tell whether its first element is `<svg>`. */
const SVG_HEAD_BYTES = 4096;
/** Text in no encoding the editor knows is still text only while control characters are rare. */
const MAX_CONTROL_SHARE = 0.05;

/** The part of a name after its last dot, in lower case; empty for a name without one. */
export function extensionOf(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

export function nameHint(name: string): NameHint {
  const extension = extensionOf(name);
  if (IMAGE_EXTENSIONS.has(extension)) return "image";
  return OPAQUE_EXTENSIONS.has(extension) ? "opaque" : "unknown";
}

function startsWith(bytes: Uint8Array, signature: readonly number[], offset = 0): boolean {
  if (bytes.length < offset + signature.length) return false;
  return signature.every((byte, index) => bytes[offset + index] === byte);
}

function ascii(text: string): number[] {
  return [...text].map((character) => character.charCodeAt(0));
}

function hasAscii(bytes: Uint8Array, text: string, offset = 0): boolean {
  return startsWith(bytes, ascii(text), offset);
}

function uint16(bytes: Uint8Array, offset: number): number {
  return bytes[offset] | (bytes[offset + 1] << 8);
}

function uint32(bytes: Uint8Array, offset: number): number {
  return (uint16(bytes, offset) | (uint16(bytes, offset + 2) << 16)) >>> 0;
}

/** The header sizes a bitmap's second header can have. Two letters alone would be too little. */
const BITMAP_HEADER_SIZES = new Set([12, 40, 52, 56, 64, 108, 124]);

function isBitmap(bytes: Uint8Array): boolean {
  return hasAscii(bytes, "BM") && bytes.length >= 18 && BITMAP_HEADER_SIZES.has(uint32(bytes, 14));
}

/** An icon file starts with four bytes many files share, so its first entry is checked too. */
function isIcon(bytes: Uint8Array): boolean {
  if (!startsWith(bytes, [0, 0, 1, 0]) || bytes.length < 22) return false;
  const count = uint16(bytes, 4);
  const reserved = bytes[9];
  const size = uint32(bytes, 14);
  const offset = uint32(bytes, 18);
  return count > 0 && reserved === 0 && size > 0 && offset >= 6 + 16 * count;
}

/** The brands of an ISO media file (`ftyp`): the main one, then those it is compatible with. */
function mediaBrands(bytes: Uint8Array): string[] | null {
  if (!hasAscii(bytes, "ftyp", 4) || bytes.length < 12) return null;
  const boxEnd = Math.min(bytes.length, Math.max(16, uint32BigEndian(bytes, 0)));
  const brands: string[] = [];
  for (let offset = 8; offset + 4 <= boxEnd; offset += 4) {
    // Bytes 12 to 16 hold a version, not a brand.
    if (offset === 12) continue;
    brands.push(String.fromCharCode(...bytes.subarray(offset, offset + 4)));
  }
  return brands;
}

function uint32BigEndian(bytes: Uint8Array, offset: number): number {
  return (
    ((bytes[offset] << 24) |
      (bytes[offset + 1] << 16) |
      (bytes[offset + 2] << 8) |
      bytes[offset + 3]) >>>
    0
  );
}

const HEIF_BRANDS = new Set(["heic", "heix", "hevc", "hevx", "heim", "heis", "mif1", "msf1"]);

function imageBySignature(bytes: Uint8Array): ImageFormat | null {
  if (startsWith(bytes, [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])) return "png";
  if (startsWith(bytes, [0xff, 0xd8, 0xff])) return "jpeg";
  if (hasAscii(bytes, "GIF87a") || hasAscii(bytes, "GIF89a")) return "gif";
  if (hasAscii(bytes, "RIFF") && hasAscii(bytes, "WEBP", 8)) return "webp";
  if (isBitmap(bytes)) return "bmp";
  if (isIcon(bytes)) return "ico";
  const brands = mediaBrands(bytes);
  if (brands?.some((brand) => brand === "avif" || brand === "avis")) return "avif";
  return null;
}

/** Zip files that are something more particular, by their extension. */
const ZIP_KINDS: Record<string, string> = {
  docx: "Office document",
  xlsx: "Office document",
  pptx: "Office document",
  odt: "Office document",
  ods: "Office document",
  odp: "Office document",
  epub: "EPUB book",
  jar: "Java archive",
  war: "Java archive",
  apk: "Android package",
};

/** Signatures long or odd enough that no text starts with them. */
function describeBySignature(bytes: Uint8Array, name: string): string | null {
  if (hasAscii(bytes, "%PDF-")) return "PDF document";
  if (startsWith(bytes, [0x50, 0x4b, 0x03, 0x04]) || startsWith(bytes, [0x50, 0x4b, 0x05, 0x06])) {
    return ZIP_KINDS[extensionOf(name)] ?? "ZIP archive";
  }
  if (startsWith(bytes, [0x1f, 0x8b, 0x08])) return "gzip archive";
  if (startsWith(bytes, [0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00])) return "xz archive";
  if (startsWith(bytes, [0x28, 0xb5, 0x2f, 0xfd])) return "Zstandard archive";
  if (startsWith(bytes, [0x37, 0x7a, 0xbc, 0xaf, 0x27, 0x1c])) return "7-Zip archive";
  if (startsWith(bytes, [0x52, 0x61, 0x72, 0x21, 0x1a, 0x07])) return "RAR archive";
  if (hasAscii(bytes, "ustar", 257)) return "tar archive";
  if (startsWith(bytes, [0x7f, 0x45, 0x4c, 0x46])) return "ELF program";
  if (hasAscii(bytes, "SQLite format 3\0")) return "SQLite database";
  if (startsWith(bytes, [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1])) {
    return "Office document";
  }
  if (hasAscii(bytes, "OggS")) return "Ogg media";
  if (hasAscii(bytes, "fLaC")) return "FLAC audio";
  if (hasAscii(bytes, "RIFF") && hasAscii(bytes, "WAVE", 8)) return "WAV audio";
  if (hasAscii(bytes, "RIFF") && hasAscii(bytes, "AVI ", 8)) return "AVI video";
  if (startsWith(bytes, [0x1a, 0x45, 0xdf, 0xa3])) return "Matroska or WebM video";
  if (hasAscii(bytes, "wOFF") || hasAscii(bytes, "wOF2")) return "Web font";
  if (hasAscii(bytes, "\0asm")) return "WebAssembly module";
  if (startsWith(bytes, [0x49, 0x49, 0x2a, 0x00]) || startsWith(bytes, [0x4d, 0x4d, 0x00, 0x2a])) {
    return "TIFF image";
  }
  if (hasAscii(bytes, "8BPS")) return "Photoshop image";
  if (startsWith(bytes, [0xed, 0xab, 0xee, 0xdb])) return "RPM package";
  const brands = mediaBrands(bytes);
  if (brands) {
    if (brands.some((brand) => HEIF_BRANDS.has(brand))) return "HEIF image";
    if (brands[0] === "M4A ") return "MPEG-4 audio";
    return brands[0] === "qt  " ? "QuickTime video" : "MPEG-4 video";
  }
  return null;
}

/** Signatures too short to trust unless the bytes are not text anyway. */
function describeBinary(bytes: Uint8Array): string | null {
  if (hasAscii(bytes, "MZ")) return "Windows program";
  if (hasAscii(bytes, "BZh")) return "bzip2 archive";
  if (hasAscii(bytes, "ID3")) return "MP3 audio";
  if (hasAscii(bytes, "!<arch>\n")) return "ar archive";
  if (startsWith(bytes, [0xca, 0xfe, 0xba, 0xbe])) return "Java class";
  const machO = [
    [0xfe, 0xed, 0xfa, 0xce],
    [0xfe, 0xed, 0xfa, 0xcf],
    [0xce, 0xfa, 0xed, 0xfe],
    [0xcf, 0xfa, 0xed, 0xfe],
  ];
  if (machO.some((signature) => startsWith(bytes, signature))) return "Mach-O program";
  return null;
}

/**
 * The bytes without a character cut in two at their end, which is what the first part of a
 * longer UTF-8 file may end in.
 */
export function withoutCutCharacter(bytes: Uint8Array): Uint8Array {
  for (let back = 1; back <= Math.min(3, bytes.length); back++) {
    const byte = bytes[bytes.length - back];
    // A byte in the middle of a character: its first byte is further back.
    if ((byte & 0xc0) === 0x80) continue;
    const length = byte >= 0xf0 ? 4 : byte >= 0xe0 ? 3 : byte >= 0xc0 ? 2 : 1;
    return length > back ? bytes.subarray(0, bytes.length - back) : bytes;
  }
  return bytes;
}

function isValidUtf8(bytes: Uint8Array): boolean {
  try {
    new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes);
    return true;
  } catch {
    return false;
  }
}

function isValidUtf16(bytes: Uint8Array, littleEndian: boolean, complete: boolean): boolean {
  let end = bytes.length - (bytes.length % 2);
  if (complete && end !== bytes.length) return false;
  const unit = (offset: number) =>
    littleEndian
      ? bytes[offset] | (bytes[offset + 1] << 8)
      : (bytes[offset] << 8) | bytes[offset + 1];
  // The first half of a pair cut from its second half.
  if (!complete && end >= 2 && (unit(end - 2) & 0xfc00) === 0xd800) end -= 2;
  for (let offset = 0; offset < end; offset += 2) {
    const value = unit(offset);
    if (value === 0) return false;
    if ((value & 0xfc00) === 0xdc00) return false;
    if ((value & 0xfc00) === 0xd800) {
      if (offset + 4 > end || (unit(offset + 2) & 0xfc00) !== 0xdc00) return false;
      offset += 2;
    }
  }
  return true;
}

function controlShare(bytes: Uint8Array): number {
  if (bytes.length === 0) return 0;
  let controls = 0;
  for (const byte of bytes) {
    const plain =
      byte >= 0x20 ? byte !== 0x7f : byte === 9 || byte === 10 || byte === 12 || byte === 13;
    if (!plain) controls++;
  }
  return controls / bytes.length;
}

/**
 * The encoding of text, told the way the editor tells it: by its byte order mark, else UTF-8
 * where the bytes are valid UTF-8, else one byte per character. Null for bytes that are not
 * text. `complete` says the bytes are the whole file, not just its beginning.
 */
export function detectTextEncoding(bytes: Uint8Array, complete: boolean): TextEncoding | null {
  if (startsWith(bytes, [0xef, 0xbb, 0xbf])) {
    const rest = bytes.subarray(3);
    return isValidUtf8(complete ? rest : withoutCutCharacter(rest)) ? "utf8Bom" : null;
  }
  if (startsWith(bytes, [0xff, 0xfe])) {
    return isValidUtf16(bytes.subarray(2), true, complete) ? "utf16Le" : null;
  }
  if (startsWith(bytes, [0xfe, 0xff])) {
    return isValidUtf16(bytes.subarray(2), false, complete) ? "utf16Be" : null;
  }
  if (bytes.subarray(0, BINARY_SNIFF_BYTES).includes(0)) return null;
  if (isValidUtf8(complete ? bytes : withoutCutCharacter(bytes))) return "utf8";
  return controlShare(bytes) <= MAX_CONTROL_SHARE ? "latin1" : null;
}

/** Where a document type declaration ends, past any definitions it holds in brackets. */
function doctypeEnd(text: string): number {
  let depth = 0;
  for (let index = 0; index < text.length; index++) {
    const character = text[index];
    if (character === "[") depth++;
    else if (character === "]") depth--;
    else if (character === ">" && depth <= 0) return index;
  }
  return -1;
}

/** Where the text after the first `closing` starts, or zero when there is none. */
function endOf(text: string, closing: string): number {
  const at = text.indexOf(closing);
  return at < 0 ? 0 : at + closing.length;
}

/**
 * The start tag of the first element of an XML text when that element is `<svg>`, after any
 * byte order mark, XML declaration, comments and document type; null for any other text.
 */
function svgRootTag(text: string): string | null {
  let rest = text.replace(/^\uFEFF/, "").trimStart();
  for (;;) {
    let end: number;
    if (rest.startsWith("<?")) end = endOf(rest, "?>");
    else if (rest.startsWith("<!--")) end = endOf(rest, "-->");
    else if (/^<!DOCTYPE/i.test(rest)) end = doctypeEnd(rest) + 1;
    else break;
    // Cut before it closed: what follows is not known.
    if (end <= 0) return null;
    rest = rest.slice(end).trimStart();
  }
  if (!/^<(?:[A-Za-z_][\w.-]*:)?svg[\s/>]/.test(rest)) return null;
  const close = rest.indexOf(">");
  return close < 0 ? rest : rest.slice(0, close + 1);
}

export function hasSvgRoot(text: string): boolean {
  return svgRootTag(text) !== null;
}

/** An attribute's value in a start tag. A name only counts after a space, not inside another. */
function attribute(tag: string, name: string): string | null {
  const match = new RegExp(`\\s${name}\\s*=\\s*(?:"([^"]*)"|'([^']*)')`).exec(tag);
  return match ? (match[1] ?? match[2]) : null;
}

/** A length in pixels, which is what a number without a unit is too; null for any other unit. */
function pixels(value: string | null): number | null {
  const match = value === null ? null : /^\s*([0-9]*\.?[0-9]+)\s*(px)?\s*$/.exec(value);
  const length = match ? Number(match[1]) : NaN;
  return length > 0 && Number.isFinite(length) ? length : null;
}

export interface DrawingSize {
  width: number;
  height: number;
}

/**
 * The size a drawing gives itself in its `width` and `height`, completed or replaced by its
 * `viewBox`. Null when it says nothing usable: it then takes whatever size it is shown at.
 */
export function drawingSize(bytes: Uint8Array): DrawingSize | null {
  const encoding = detectTextEncoding(bytes.subarray(0, SVG_HEAD_BYTES), false) ?? "utf8";
  const tag = svgRootTag(headText(bytes, encoding));
  if (!tag) return null;
  const width = pixels(attribute(tag, "width"));
  const height = pixels(attribute(tag, "height"));
  const box = (attribute(tag, "viewBox") ?? "")
    .trim()
    .split(/[\s,]+/)
    .map(Number);
  const [boxWidth, boxHeight] = box.length === 4 ? box.slice(2) : [NaN, NaN];
  const hasBox = boxWidth > 0 && boxHeight > 0 && Number.isFinite(boxWidth + boxHeight);
  const round = (length: number) => Math.max(1, Math.round(length));
  if (width && height) return { width: round(width), height: round(height) };
  if (!hasBox) return null;
  if (width) return { width: round(width), height: round((width * boxHeight) / boxWidth) };
  if (height) return { width: round((height * boxWidth) / boxHeight), height: round(height) };
  return { width: round(boxWidth), height: round(boxHeight) };
}

function headText(bytes: Uint8Array, encoding: TextEncoding): string {
  const head = bytes.subarray(0, SVG_HEAD_BYTES);
  if (encoding === "utf16Le" || encoding === "utf16Be") {
    const label = encoding === "utf16Le" ? "utf-16le" : "utf-16be";
    return new TextDecoder(label).decode(head.subarray(0, head.length - (head.length % 2)));
  }
  // The marks, names and brackets looked for are the same in UTF-8 and one byte per character.
  return new TextDecoder("utf-8").decode(head);
}

/**
 * What `bytes`, the beginning of the file called `name`, are. `complete` says they are the
 * whole file.
 */
export function sniff(bytes: Uint8Array, name: string, complete: boolean): Sniffed {
  if (bytes.length === 0) return { kind: "empty" };
  const image = imageBySignature(bytes);
  if (image) return { kind: "image", format: image };
  const described = describeBySignature(bytes, name);
  if (described) return { kind: "binary", description: described };
  const encoding = detectTextEncoding(bytes, complete);
  if (!encoding) return { kind: "binary", description: describeBinary(bytes) };
  // A drawing is XML that says so, in a file not named as some other kind of text.
  const extension = extensionOf(name);
  if ((extension === "svg" || extension === "") && hasSvgRoot(headText(bytes, encoding))) {
    return { kind: "image", format: "svg" };
  }
  return { kind: "text", encoding };
}
