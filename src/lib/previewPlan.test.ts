import { describe, expect, it } from "vitest";
import {
  HARD_LIMIT_BYTES,
  SIGNATURE_BYTES,
  capsOf,
  changedSinceListed,
  firstStep,
  fitsThumbnail,
  listsExactTimes,
  nextStep,
  paneStorage,
  problemOf,
  selectionKey,
  storageKind,
  subjectOf,
  summarize,
  type Caps,
} from "./previewPlan";
import { DEFAULT_FILE_PREVIEW } from "./previewSettings";
import type { AppError, FileEntry } from "./types";

const MEBIBYTE = 1024 * 1024;
const caps: Caps = { imageBytes: 16 * MEBIBYTE, textBytes: 256 * 1024 };
const encoder = new TextEncoder();
const PNG_START = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

function entry(name: string, size: number, more: Partial<FileEntry> = {}): FileEntry {
  return {
    name,
    path: `/srv/${name}`,
    kind: "file",
    size,
    modified: 1_700_000_000,
    permissions: 0o644,
    owner: "poros",
    group: "staff",
    hidden: false,
    ...more,
  };
}

function png(length: number): Uint8Array {
  const bytes = new Uint8Array(length);
  bytes.set(PNG_START);
  return bytes;
}

/** A PNG whose header says it is `width` by `height` pixels. */
function pngOf(width: number, height: number, length = 64): Uint8Array {
  const bytes = png(length);
  bytes.set([0, 0, 0, 13, 0x49, 0x48, 0x44, 0x52], 8);
  const header = new DataView(bytes.buffer);
  header.setUint32(16, width);
  header.setUint32(20, height);
  return bytes;
}

describe("capsOf", () => {
  it("reads the caps from the settings, which default to 16 MiB and 256 KiB", () => {
    expect(capsOf(DEFAULT_FILE_PREVIEW)).toEqual(caps);
  });
});

describe("storageKind", () => {
  it("tells this computer, servers and storage that charges for reads apart", () => {
    expect(storageKind(undefined)).toBe("local");
    for (const protocol of ["sftp", "ftp", "ftps", "ftpsImplicit", "webDav", "webDavs"] as const) {
      expect(storageKind(protocol)).toBe("server");
    }
    for (const protocol of ["s3", "backblazeB2", "googleDrive", "oneDrive", "box"] as const) {
      expect(storageKind(protocol)).toBe("metered");
    }
  });

  it("trusts listed times everywhere but on FTP", () => {
    expect(listsExactTimes(undefined)).toBe(true);
    expect(listsExactTimes("sftp")).toBe(true);
    expect(listsExactTimes("s3")).toBe(true);
    expect(listsExactTimes("ftp")).toBe(false);
    expect(listsExactTimes("ftpsImplicit")).toBe(false);
  });
});

describe("subjectOf", () => {
  const file = entry("a.txt", 5);
  const folder = entry("photos", 0, { kind: "dir" });

  it("is the item under the cursor, by what it is", () => {
    expect(subjectOf(file, [])).toEqual({ kind: "file", entry: file });
    expect(subjectOf(folder, [folder])).toEqual({ kind: "folder", entry: folder });
    const linkToFolder = entry("current", 9, { kind: "symlink", linkTarget: "dir" });
    expect(subjectOf(linkToFolder, []).kind).toBe("folder");
    const linkToFile = entry("latest.log", 9, { kind: "symlink", linkTarget: "file" });
    expect(subjectOf(linkToFile, []).kind).toBe("file");
    const broken = entry("gone", 9, { kind: "symlink", linkTarget: "broken" });
    expect(subjectOf(broken, []).kind).toBe("special");
    expect(subjectOf(entry("sda", 0, { kind: "other" }), []).kind).toBe("special");
    expect(subjectOf(undefined, [])).toEqual({ kind: "nothing" });
  });

  it("is the selection once several items are selected, wherever the cursor is", () => {
    const other = entry("b.txt", 7);
    expect(subjectOf(file, [file, other])).toEqual({ kind: "selection", entries: [file, other] });
    // One selected item with the cursor elsewhere: the cursor decides.
    expect(subjectOf(other, [file])).toEqual({ kind: "file", entry: other });
    expect(subjectOf(undefined, [file])).toEqual({ kind: "file", entry: file });
  });
});

describe("paneStorage", () => {
  it("counts a session not known here yet as one whose reads are paid for", () => {
    expect(paneStorage(false, undefined)).toBe("local");
    expect(paneStorage(true, undefined)).toBe("metered");
    expect(paneStorage(true, "sftp")).toBe("server");
    expect(paneStorage(true, "s3")).toBe("metered");
  });
});

describe("firstStep", () => {
  it("reads nothing of a file this computer lists as empty", () => {
    expect(firstStep(entry("empty.log", 0), caps, false)).toEqual({ step: "show", view: "empty" });
  });

  it("reads a file a server lists as 0 bytes, which may be a size it does not know", () => {
    for (const sizes of ["listed", "remembered"] as const) {
      expect(firstStep(entry("Budget", 0), caps, false, sizes)).toEqual({
        step: "read",
        length: caps.textBytes,
      });
    }
  });

  it("decides nothing by the size of a remembered listing, which the file may have outgrown", () => {
    const listedLarge = entry("scan.png", 40 * MEBIBYTE);
    expect(firstStep(listedLarge, caps, false, "remembered")).toEqual({
      step: "read",
      length: SIGNATURE_BYTES,
    });
    expect(firstStep(entry("photo.jpg", 3 * MEBIBYTE), caps, false, "remembered")).toEqual({
      step: "read",
      length: 3 * MEBIBYTE,
    });
    // A server's own size for a picture still decides.
    expect(firstStep(listedLarge, caps, false, "listed")).toMatchObject({ view: "tooLarge" });
  });

  it("reads a picture whole, a text as far as it is shown, anything else by its signature", () => {
    expect(firstStep(entry("photo.jpg", 3 * MEBIBYTE), caps, false)).toEqual({
      step: "read",
      length: 3 * MEBIBYTE,
    });
    expect(firstStep(entry("server.log", 90 * MEBIBYTE), caps, false)).toEqual({
      step: "read",
      length: caps.textBytes,
    });
    expect(firstStep(entry("notes.md", 1200), caps, false)).toEqual({ step: "read", length: 1200 });
    expect(firstStep(entry("backup.tar.gz", 4000 * MEBIBYTE), caps, false)).toEqual({
      step: "read",
      length: SIGNATURE_BYTES,
    });
    expect(firstStep(entry("tiny.zip", 22), caps, false)).toEqual({ step: "read", length: 22 });
  });

  it("stops at a picture over the cap until the user asks for it anyway", () => {
    const large = entry("scan.png", 40 * MEBIBYTE);
    expect(firstStep(large, caps, false)).toEqual({
      step: "show",
      view: "tooLarge",
      canLoad: true,
      dimensions: null,
    });
    expect(firstStep(large, caps, true)).toEqual({ step: "read", length: 40 * MEBIBYTE });
    expect(firstStep(entry("scan.png", caps.imageBytes), caps, false).step).toBe("read");
    // More than a read returns, whatever the user asks for.
    const huge = entry("poster.png", HARD_LIMIT_BYTES + 1);
    expect(firstStep(huge, caps, true)).toEqual({
      step: "show",
      view: "tooLarge",
      canLoad: false,
      dimensions: null,
    });
  });

  it("does not go by the size a link is listed with, which is the link's own", () => {
    const link = entry("latest.png", 17, { kind: "symlink", linkTarget: "file" });
    expect(firstStep(link, caps, false)).toEqual({ step: "read", length: caps.textBytes });
  });
});

describe("nextStep", () => {
  it("shows a picture once all of it is there", () => {
    expect(nextStep("photo.png", png(5000), 5000, caps, false)).toEqual({
      step: "show",
      view: "image",
      format: "png",
    });
  });

  it("reads the rest of a picture found behind another name, within the cap", () => {
    // Named as an archive, so only its signature was read.
    expect(nextStep("photo.zip", png(SIGNATURE_BYTES), 80_000, caps, false)).toEqual({
      step: "read",
      length: 80_000,
    });
    expect(nextStep("photo.zip", png(SIGNATURE_BYTES), 40 * MEBIBYTE, caps, false)).toEqual({
      step: "show",
      view: "tooLarge",
      canLoad: true,
      dimensions: null,
    });
    expect(nextStep("photo.zip", png(SIGNATURE_BYTES), 40 * MEBIBYTE, caps, true)).toEqual({
      step: "read",
      length: 40 * MEBIBYTE,
    });
  });

  it("does not show a picture with more pixels than decode safely, small as its file may be", () => {
    // 48 megapixels: shown when asked for.
    const large = pngOf(8000, 6000);
    expect(nextStep("pano.png", large, large.length, caps, false)).toEqual({
      step: "show",
      view: "tooLarge",
      canLoad: true,
      dimensions: { width: 8000, height: 6000 },
    });
    expect(nextStep("pano.png", large, large.length, caps, true)).toEqual({
      step: "show",
      view: "image",
      format: "png",
    });
    // 400 megapixels: never, whatever the user asks for.
    const bomb = pngOf(20_000, 20_000);
    expect(nextStep("tiny.png", bomb, bomb.length, caps, true)).toEqual({
      step: "show",
      view: "tooLarge",
      canLoad: false,
      dimensions: { width: 20_000, height: 20_000 },
    });
    const camera = pngOf(6000, 4000);
    expect(nextStep("camera.png", camera, camera.length, caps, false)).toMatchObject({
      view: "image",
    });
  });

  it("shows the beginning of a long text and says it is only the beginning", () => {
    const start = new Uint8Array(caps.textBytes).fill(0x61);
    expect(nextStep("server.log", start, 90 * MEBIBYTE, caps, false)).toEqual({
      step: "show",
      view: "text",
      encoding: "utf8",
      length: caps.textBytes,
      partial: true,
    });
    const short = encoder.encode("short\n");
    expect(nextStep("notes.md", short, short.length, caps, false)).toEqual({
      step: "show",
      view: "text",
      encoding: "utf8",
      length: short.length,
      partial: false,
    });
  });

  it("reads on when a file named as binary turns out to be text", () => {
    const start = new Uint8Array(SIGNATURE_BYTES).fill(0x61);
    expect(nextStep("notes.pdf", start, 9000, caps, false)).toEqual({ step: "read", length: 9000 });
    expect(nextStep("notes.pdf", start, 90 * MEBIBYTE, caps, false)).toEqual({
      step: "read",
      length: caps.textBytes,
    });
  });

  it("shows only as much text as the cap of a file read whole for being named a picture", () => {
    const text = new Uint8Array(3 * MEBIBYTE).fill(0x61);
    expect(nextStep("fake.png", text, text.length, caps, false)).toEqual({
      step: "show",
      view: "text",
      encoding: "utf8",
      length: caps.textBytes,
      partial: true,
    });
  });

  it("reads a drawing whole before showing it", () => {
    const head = encoder.encode('<svg xmlns="http://www.w3.org/2000/svg">' + " ".repeat(600));
    const start = head.subarray(0, 512);
    expect(nextStep("logo", start, 700_000, caps, false)).toEqual({
      step: "read",
      length: 700_000,
    });
    const whole = new Uint8Array(700_000).fill(0x20);
    whole.set(head);
    expect(nextStep("logo.svg", whole, whole.length, caps, false)).toEqual({
      step: "show",
      view: "image",
      format: "svg",
    });
  });

  it("shows the details of anything else", () => {
    const zip = Uint8Array.from([0x50, 0x4b, 0x03, 0x04, 20, 0, 0, 0]);
    expect(nextStep("backup.zip", zip, 5 * MEBIBYTE, caps, false)).toEqual({
      step: "show",
      view: "binary",
      description: "ZIP archive",
    });
    expect(nextStep("gone.txt", new Uint8Array(0), 0, caps, false)).toEqual({
      step: "show",
      view: "empty",
    });
  });
});

describe("fitsThumbnail", () => {
  it("draws pictures up to 25 megapixels, and those whose header does not say", () => {
    expect(fitsThumbnail(pngOf(6000, 4000), "png")).toBe(true);
    expect(fitsThumbnail(pngOf(6000, 5000), "png")).toBe(false);
    expect(fitsThumbnail(png(64), "png")).toBe(true);
    expect(fitsThumbnail(encoder.encode("<svg width='99999' height='99999'/>"), "svg")).toBe(true);
  });
});

describe("changedSinceListed", () => {
  const listed = entry("report.txt", 1000, { modified: 1_700_000_000 });

  it("notices another size, or another time where times can be trusted", () => {
    const same = { size: 1000, modified: 1_700_000_000_400 };
    expect(changedSinceListed(listed, same, true)).toBe(false);
    expect(changedSinceListed(listed, { ...same, size: 1001 }, true)).toBe(true);
    expect(changedSinceListed(listed, { size: 1000, modified: 1_700_000_090_000 }, true)).toBe(
      true,
    );
    // FTP lists times too coarsely to tell.
    expect(changedSinceListed(listed, { size: 1000, modified: 1_700_000_090_000 }, false)).toBe(
      false,
    );
    expect(changedSinceListed(listed, { ...same, size: 999 }, false)).toBe(true);
    expect(changedSinceListed(listed, { size: 1000, modified: null }, true)).toBe(false);
    expect(changedSinceListed({ ...listed, modified: null }, same, true)).toBe(false);
  });

  it("says nothing of a link, which is listed with its own size", () => {
    const link = entry("latest", 12, { kind: "symlink", linkTarget: "file" });
    expect(changedSinceListed(link, { size: 9000, modified: 1 }, true)).toBe(false);
  });

  it("takes a server's 0 for no size at all, but a remembered listing's sizes at their word", () => {
    const unsized = entry("Budget", 0, { modified: 1_700_000_000 });
    const now = { size: 5000, modified: 1_700_000_000_000 };
    expect(changedSinceListed(unsized, now, true, "listed")).toBe(false);
    expect(changedSinceListed(unsized, now, true, "exact")).toBe(true);
    expect(changedSinceListed(unsized, now, true, "remembered")).toBe(true);
    const remembered = entry("report.txt", 1000, { modified: 1_700_000_000 });
    expect(
      changedSinceListed(
        remembered,
        { size: 1000, modified: 1_700_000_500_000 },
        true,
        "remembered",
      ),
    ).toBe(true);
  });
});

describe("problemOf", () => {
  const error = (kind: AppError["kind"]): AppError => ({ kind, message: "" });

  it("sorts failures into the states the preview shows", () => {
    expect(problemOf(error("permissionDenied"), false)).toBe("denied");
    expect(problemOf(error("notFound"), false)).toBe("gone");
    expect(problemOf(error("busy"), false)).toBe("busy");
    expect(problemOf(error("disconnected"), false)).toBe("lost");
    expect(problemOf(error("connection"), false)).toBe("lost");
    expect(problemOf(error("timeout"), true)).toBe("lost");
    expect(problemOf(error("sessionNotFound"), false)).toBe("lost");
    // A vault's session ends when the vault is locked.
    expect(problemOf(error("sessionNotFound"), true)).toBe("locked");
    expect(problemOf(error("io"), false)).toBe("other");
  });
});

describe("summarize", () => {
  it("counts folders and files, adds up sizes and picks out the pictures", () => {
    const entries = [
      entry("photos", 4096, { kind: "dir" }),
      entry("linked", 9, { kind: "symlink", linkTarget: "dir" }),
      entry("a.png", 2000),
      entry("b.JPG", 3000),
      entry("notes.txt", 500),
      entry("huge.png", 40 * MEBIBYTE),
      entry("empty.gif", 0),
      entry("link.webp", 11, { kind: "symlink", linkTarget: "file" }),
      entry("broken.png", 11, { kind: "symlink", linkTarget: "broken" }),
    ];
    const summary = summarize(entries, caps);
    expect(summary.folders).toBe(2);
    expect(summary.files).toBe(7);
    // Links count for nothing: their listed size is their own.
    expect(summary.bytes).toBe(2000 + 3000 + 500 + 40 * MEBIBYTE);
    expect(summary.pictures.map((picture) => picture.name)).toEqual([
      "a.png",
      "b.JPG",
      "link.webp",
    ]);
  });
});

describe("selectionKey", () => {
  it("changes with the items selected and with what they are", () => {
    const first = [entry("a.png", 1), entry("b.png", 2)];
    expect(selectionKey(first)).toBe(selectionKey([...first]));
    expect(selectionKey(first)).not.toBe(selectionKey([first[0]]));
    expect(selectionKey(first)).not.toBe(selectionKey([first[0], entry("b.png", 3)]));
  });
});
