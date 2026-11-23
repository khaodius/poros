import { describe, expect, it } from "vitest";
import {
  detectTextEncoding,
  extensionOf,
  hasSvgRoot,
  nameHint,
  sniff,
  withoutCutCharacter,
} from "./previewSniff";

const encoder = new TextEncoder();

function bytes(...parts: (number[] | string)[]): Uint8Array {
  return Uint8Array.from(
    parts.flatMap((part) => (typeof part === "string" ? [...encoder.encode(part)] : part)),
  );
}

/** `values` as 16-bit code units, the least significant byte first unless `bigEndian`. */
function utf16(text: string, bigEndian = false): number[] {
  return [...text].flatMap((character) => {
    const unit = character.charCodeAt(0);
    return bigEndian ? [unit >> 8, unit & 0xff] : [unit & 0xff, unit >> 8];
  });
}

const PNG = bytes([0x89], "PNG\r\n", [0x1a, 0x0a, 0, 0, 0, 13], "IHDR");
const JPEG = bytes([0xff, 0xd8, 0xff, 0xe0, 0, 16], "JFIF", [0]);
const WEBP = bytes("RIFF", [0x24, 0, 0, 0], "WEBPVP8 ");
const BMP = bytes("BM", [0x46, 0, 0, 0, 0, 0, 0, 0, 0x36, 0, 0, 0, 40, 0, 0, 0, 1, 0, 0, 0]);
const ICO = bytes([0, 0, 1, 0, 1, 0, 16, 16, 0, 0, 1, 0, 32, 0, 0x68, 4, 0, 0, 22, 0, 0, 0]);
const AVIF = bytes([0, 0, 0, 0x1c], "ftypavif", [0, 0, 0, 0], "avifmif1miaf");
const SVG = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><rect width="8"/></svg>';

describe("sniff", () => {
  it("tells each picture the web view decodes by its signature", () => {
    const pictures: [Uint8Array, string][] = [
      [PNG, "png"],
      [JPEG, "jpeg"],
      [bytes("GIF87a", [1, 0, 1, 0]), "gif"],
      [bytes("GIF89a", [1, 0, 1, 0]), "gif"],
      [WEBP, "webp"],
      [BMP, "bmp"],
      [ICO, "ico"],
      [AVIF, "avif"],
      [bytes([0, 0, 0, 0x1c], "ftypmif1", [0, 0, 0, 0], "mif1avifmiaf"), "avif"],
    ];
    for (const [content, format] of pictures) {
      // The name says nothing true about any of them.
      expect(sniff(content, "download.bin", true)).toEqual({ kind: "image", format });
    }
  });

  it("is not taken in by a name", () => {
    // A picture named as something else is still a picture.
    expect(sniff(PNG, "backup.zip", true)).toEqual({ kind: "image", format: "png" });
    expect(sniff(JPEG, "notes.txt", false)).toEqual({ kind: "image", format: "jpeg" });
    expect(sniff(PNG, "drawing.svg", true)).toEqual({ kind: "image", format: "png" });
    // Text named as a picture is still text.
    expect(sniff(bytes("not a picture at all\n"), "photo.png", true)).toEqual({
      kind: "text",
      encoding: "utf8",
    });
    expect(sniff(bytes("plain words\n"), "logo.svg", true)).toEqual({
      kind: "text",
      encoding: "utf8",
    });
    // And an archive named as text is no text.
    const zip = bytes("PK", [3, 4, 20, 0, 0, 0, 8, 0], "rest");
    expect(sniff(zip, "readme.txt", false)).toEqual({ kind: "binary", description: "ZIP archive" });
    expect(sniff(zip, "report.docx", false)).toEqual({
      kind: "binary",
      description: "Office document",
    });
  });

  it("does not mistake short signatures for pictures", () => {
    // Text that starts with the two letters of a bitmap.
    expect(sniff(bytes("BMW service history, 2019 to 2024\n"), "cars.txt", true)).toEqual({
      kind: "text",
      encoding: "utf8",
    });
    // The four bytes an icon starts with, without an icon's first entry after them.
    expect(
      sniff(
        bytes([0, 0, 1, 0, 0, 0, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9]),
        "a.ico",
        true,
      ),
    ).toEqual({
      kind: "binary",
      description: null,
    });
    // A HEIF picture shares its container with AVIF but is not one.
    const heic = bytes([0, 0, 0, 0x18], "ftypheic", [0, 0, 0, 0], "mif1heic");
    expect(sniff(heic, "photo.heic", false)).toEqual({ kind: "binary", description: "HEIF image" });
    const video = bytes([0, 0, 0, 0x18], "ftypisom", [0, 0, 2, 0], "isomiso2");
    expect(sniff(video, "clip.png", false)).toEqual({
      kind: "binary",
      description: "MPEG-4 video",
    });
  });

  it("tells a drawing by its first element, with and without an XML declaration", () => {
    const svg = { kind: "image", format: "svg" };
    expect(sniff(bytes(SVG), "logo.svg", true)).toEqual(svg);
    expect(
      sniff(bytes(`<?xml version="1.0" encoding="UTF-8"?>\n${SVG}`), "logo.svg", true),
    ).toEqual(svg);
    expect(
      sniff(bytes([0xef, 0xbb, 0xbf], `<?xml version="1.0"?>${SVG}`), "logo.svg", true),
    ).toEqual(svg);
    const withProlog =
      '<?xml version="1.0"?>\n<!-- Made by hand -->\n' +
      '<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "svg11.dtd" [ <!ENTITY ns "x"> ]>\n' +
      SVG;
    expect(sniff(bytes(withProlog), "logo.svg", true)).toEqual(svg);
    // The beginning of a longer drawing.
    expect(sniff(bytes(SVG.slice(0, 40)), "logo.svg", false)).toEqual(svg);
    expect(sniff(bytes([0xff, 0xfe], utf16(SVG)), "logo.svg", true)).toEqual(svg);
    // A file with no extension goes by what is in it.
    expect(sniff(bytes(SVG), "logo", true)).toEqual(svg);
  });

  it("shows a drawing as source when its name says it is other text, or it is not one", () => {
    const text = { kind: "text", encoding: "utf8" };
    expect(sniff(bytes(SVG), "icon.xml", true)).toEqual(text);
    expect(sniff(bytes(SVG), "icon.txt", true)).toEqual(text);
    expect(sniff(bytes(`<html><body>${SVG}</body></html>`), "page.svg", true)).toEqual(text);
    expect(sniff(bytes('<?xml version="1.0"?><feed></feed>'), "feed.svg", true)).toEqual(text);
    // Cut inside a comment: what follows is not known.
    expect(sniff(bytes("<!-- a long comment that"), "cut.svg", false)).toEqual(text);
  });

  it("tells text from binary the way the editor does", () => {
    expect(sniff(bytes("fn main() {}\n"), "main.rs", true)).toEqual({
      kind: "text",
      encoding: "utf8",
    });
    expect(sniff(bytes("naïve café ✓\n"), "words", true)).toEqual({
      kind: "text",
      encoding: "utf8",
    });
    // Not valid UTF-8, but one byte per character reads as text.
    expect(sniff(bytes("caf", [0xe9], " au lait\n"), "menu.txt", true)).toEqual({
      kind: "text",
      encoding: "latin1",
    });
    // A NUL byte marks a binary file.
    expect(sniff(bytes("abc", [0], "def"), "data.txt", true)).toEqual({
      kind: "binary",
      description: null,
    });
    // Neither UTF-8 nor mostly printable.
    expect(sniff(bytes([0x01, 0x02, 0x9f, 0x03, 0x04, 0xe9, 0x05, 0x06]), "blob", true)).toEqual({
      kind: "binary",
      description: null,
    });
    expect(sniff(new Uint8Array(0), "empty.txt", true)).toEqual({ kind: "empty" });
  });

  it("reads byte order marks", () => {
    expect(sniff(bytes([0xef, 0xbb, 0xbf], "with a mark\n"), "a.txt", true)).toEqual({
      kind: "text",
      encoding: "utf8Bom",
    });
    expect(sniff(bytes([0xff, 0xfe], utf16("wide\n")), "a.txt", true)).toEqual({
      kind: "text",
      encoding: "utf16Le",
    });
    expect(sniff(bytes([0xfe, 0xff], utf16("wide\n", true)), "a.txt", true)).toEqual({
      kind: "text",
      encoding: "utf16Be",
    });
    // A mark before bytes that are not what it promises.
    expect(sniff(bytes([0xef, 0xbb, 0xbf, 0xff, 0xff]), "a.txt", true).kind).toBe("binary");
    expect(sniff(bytes([0xff, 0xfe, 0x00, 0xdc, 0x41, 0x00]), "a.txt", true).kind).toBe("binary");
    expect(sniff(bytes([0xff, 0xfe, 0x41, 0x00, 0x42]), "a.txt", true).kind).toBe("binary");
  });

  it("names what a binary file is where its signature tells", () => {
    const described: [Uint8Array, string, string][] = [
      [bytes("%PDF-1.7\n"), "manual.pdf", "PDF document"],
      [bytes([0x1f, 0x8b, 0x08, 0]), "logs.tar.gz", "gzip archive"],
      [bytes([0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00]), "a.xz", "xz archive"],
      [bytes([0x28, 0xb5, 0x2f, 0xfd]), "a.zst", "Zstandard archive"],
      [bytes([0x37, 0x7a, 0xbc, 0xaf, 0x27, 0x1c]), "a.7z", "7-Zip archive"],
      [bytes([0x7f], "ELF", [2, 1, 1, 0]), "server", "ELF program"],
      [bytes("SQLite format 3", [0]), "app.db", "SQLite database"],
      [bytes("MZ", [0x90, 0, 3, 0, 0, 0]), "setup.exe", "Windows program"],
      [bytes("RIFF", [0, 0, 0, 0], "WAVEfmt "), "sound.wav", "WAV audio"],
      [bytes("II*", [0, 8, 0, 0, 0]), "scan.tif", "TIFF image"],
    ];
    for (const [content, name, description] of described) {
      expect(sniff(content, name, false)).toEqual({ kind: "binary", description });
    }
    // Text that happens to start with a short signature stays text.
    expect(sniff(bytes("MZ is how old programs start\n"), "notes.txt", true).kind).toBe("text");
  });
});

describe("detectTextEncoding", () => {
  it("allows a character cut in two at the end of the first part of a file", () => {
    const text = bytes("price: 5 €");
    const cut = text.subarray(0, text.length - 1);
    expect(detectTextEncoding(cut, false)).toBe("utf8");
    // The same bytes as a whole file are not valid UTF-8.
    expect(detectTextEncoding(cut, true)).toBe("latin1");
    const wide = bytes([0xff, 0xfe], utf16("ab"), [0x3d, 0xd8]);
    expect(detectTextEncoding(wide, false)).toBe("utf16Le");
    expect(detectTextEncoding(wide, true)).toBeNull();
    expect(detectTextEncoding(wide.subarray(0, wide.length - 1), false)).toBe("utf16Le");
  });

  it("looks for NUL bytes only as far as the editor does", () => {
    const long = new Uint8Array(9000).fill(0x61);
    long[8500] = 0;
    expect(detectTextEncoding(long, true)).toBe("utf8");
    long[100] = 0;
    expect(detectTextEncoding(long, true)).toBeNull();
  });
});

describe("withoutCutCharacter", () => {
  it("drops only the bytes of a character whose end is missing", () => {
    const euro = [0xe2, 0x82, 0xac];
    expect([...withoutCutCharacter(bytes("a", euro))]).toEqual([0x61, ...euro]);
    expect([...withoutCutCharacter(bytes("a", euro.slice(0, 2)))]).toEqual([0x61]);
    expect([...withoutCutCharacter(bytes("a", euro.slice(0, 1)))]).toEqual([0x61]);
    expect([...withoutCutCharacter(bytes("ab"))]).toEqual([0x61, 0x62]);
    const emoji = [0xf0, 0x9f, 0x98, 0x80];
    expect([...withoutCutCharacter(bytes(emoji))]).toEqual(emoji);
    expect([...withoutCutCharacter(bytes("a", emoji.slice(0, 3)))]).toEqual([0x61]);
    expect(withoutCutCharacter(new Uint8Array(0)).length).toBe(0);
  });
});

describe("hasSvgRoot", () => {
  it("skips what may come before the first element", () => {
    expect(hasSvgRoot("  \n<svg>")).toBe(true);
    expect(hasSvgRoot("<svg/>")).toBe(true);
    expect(hasSvgRoot('<svg:svg xmlns:svg="http://www.w3.org/2000/svg">')).toBe(true);
    expect(hasSvgRoot("<!-- one --><!-- two --><svg ")).toBe(true);
    expect(hasSvgRoot("<svgs>")).toBe(false);
    expect(hasSvgRoot("<p><svg></svg></p>")).toBe(false);
    expect(hasSvgRoot("<?xml version")).toBe(false);
    expect(hasSvgRoot("")).toBe(false);
  });
});

describe("nameHint", () => {
  it("goes by the extension, whatever its case", () => {
    expect(nameHint("Photo.JPG")).toBe("image");
    expect(nameHint("logo.svg")).toBe("image");
    expect(nameHint("backup.tar.gz")).toBe("opaque");
    expect(nameHint("film.mkv")).toBe("opaque");
    expect(nameHint("main.rs")).toBe("unknown");
    expect(nameHint("Makefile")).toBe("unknown");
    // A leading dot marks a hidden file, not an extension.
    expect(nameHint(".png")).toBe("unknown");
    expect(extensionOf(".bashrc")).toBe("");
    expect(extensionOf("a.b.C")).toBe("c");
  });
});
