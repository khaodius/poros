import { describe, expect, it, vi } from "vitest";
import { loadFileView, viewWeight, type LoadOptions } from "./previewLoad";
import { HEX_BYTES, SIGNATURE_BYTES, type Caps } from "./previewPlan";
import { PreviewCancelled } from "./previewScheduler";
import type { FileEntry, PreviewRead } from "./types";

const MEBIBYTE = 1024 * 1024;
const caps: Caps = { imageBytes: 16 * MEBIBYTE, textBytes: 256 * 1024 };
const encoder = new TextEncoder();
const LISTED_AT = 1_700_000_000;

function entry(name: string, size: number, more: Partial<FileEntry> = {}): FileEntry {
  return {
    name,
    path: `/srv/${name}`,
    kind: "file",
    size,
    modified: LISTED_AT,
    permissions: null,
    owner: null,
    group: null,
    hidden: false,
    ...more,
  };
}

function png(length: number): Uint8Array {
  const bytes = new Uint8Array(length);
  bytes.set([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  return bytes;
}

/** A file on a server: answers each read with as much of `contents` as was asked for. */
function server(contents: Uint8Array, modified: number | null = LISTED_AT * 1000) {
  const read = vi.fn(async (length: number): Promise<PreviewRead> => ({
    stamp: { size: contents.length, modified },
    bytes: contents.subarray(0, length),
  }));
  const options = (more: Partial<LoadOptions> = {}): LoadOptions => ({
    read,
    caps,
    loadAnyway: false,
    exactTimes: true,
    ...more,
  });
  return { read, options };
}

const never = new AbortController().signal;

describe("loadFileView", () => {
  it("reads a picture whole, in one read", async () => {
    const contents = png(70_000);
    const { read, options } = server(contents);
    const view = await loadFileView(entry("photo.png", 70_000), options(), never);
    expect(view).toEqual({ view: "image", format: "png", bytes: contents, changed: false });
    expect(read).toHaveBeenCalledTimes(1);
    expect(read.mock.calls[0][0]).toBe(70_000);
  });

  it("reads only the beginning of a long text and ends it at a whole line", async () => {
    const line = "2024-05-01 12:00:00 GET /index.html 200\n";
    const contents = encoder.encode(line.repeat(20_000));
    const { read, options } = server(contents);
    const view = await loadFileView(entry("access.log", contents.length), options(), never);
    expect(read).toHaveBeenCalledTimes(1);
    expect(read.mock.calls[0][0]).toBe(caps.textBytes);
    if (view.view !== "text") throw new Error(`shown as ${view.view}`);
    expect(view.partial).toBe(true);
    expect(view.encoding).toBe("utf8");
    expect(view.size).toBe(contents.length);
    expect(view.text.endsWith(line)).toBe(true);
    expect(view.text.length).toBeLessThanOrEqual(caps.textBytes);
    expect(view.text.length).toBeGreaterThan(caps.textBytes - line.length);
  });

  it("shows a short text whole", async () => {
    const contents = encoder.encode("# Title\n\nwithout a line break at the end");
    const { options } = server(contents);
    const view = await loadFileView(entry("README.md", contents.length), options(), never);
    expect(view).toMatchObject({
      view: "text",
      text: "# Title\n\nwithout a line break at the end",
      partial: false,
    });
  });

  it("reads no more than the signature of a file that cannot be shown", async () => {
    const contents = new Uint8Array(900 * 1024);
    contents.set([0x50, 0x4b, 0x03, 0x04]);
    const { read, options } = server(contents);
    const view = await loadFileView(entry("site.zip", contents.length), options(), never);
    expect(read).toHaveBeenCalledTimes(1);
    expect(read.mock.calls[0][0]).toBe(SIGNATURE_BYTES);
    if (view.view !== "binary") throw new Error(`shown as ${view.view}`);
    expect(view.description).toBe("ZIP archive");
    expect(view.head.length).toBe(HEX_BYTES);
  });

  it("reads nothing of an empty file", async () => {
    const { read, options } = server(new Uint8Array(0));
    expect(await loadFileView(entry("empty.txt", 0), options(), never)).toEqual({ view: "empty" });
    expect(read).not.toHaveBeenCalled();
  });

  it("follows the bytes, not the name: a picture named as an archive", async () => {
    const contents = png(300_000);
    const { read, options } = server(contents);
    const view = await loadFileView(entry("photo.zip", contents.length), options(), never);
    expect(view.view).toBe("image");
    expect(read.mock.calls.map(([length]) => length)).toEqual([SIGNATURE_BYTES, 300_000]);
  });

  it("follows the bytes, not the name: text named as a picture or a document", async () => {
    const contents = encoder.encode("just words\n".repeat(50));
    const asPicture = server(contents);
    const first = await loadFileView(
      entry("photo.png", contents.length),
      asPicture.options(),
      never,
    );
    expect(first).toMatchObject({ view: "text", partial: false });
    expect(asPicture.read).toHaveBeenCalledTimes(1);

    const asDocument = server(encoder.encode("a".repeat(2000)));
    const second = await loadFileView(entry("notes.pdf", 2000), asDocument.options(), never);
    expect(second).toMatchObject({ view: "text", text: "a".repeat(2000) });
    expect(asDocument.read.mock.calls.map(([length]) => length)).toEqual([SIGNATURE_BYTES, 2000]);
  });

  it("reads a file a server lists as 0 bytes, and goes by the size it has now", async () => {
    const contents = encoder.encode("A document the server measures only when it is read");
    const { read, options } = server(contents);
    const view = await loadFileView(entry("Budget", 0), options({ sizes: "listed" }), never);
    expect(view).toMatchObject({ view: "text", partial: false, changed: false });
    expect(read).toHaveBeenCalledTimes(1);
    const empty = server(new Uint8Array(0));
    expect(
      await loadFileView(entry("empty.log", 0), empty.options({ sizes: "listed" }), never),
    ).toEqual({ view: "empty" });
    expect(empty.read).toHaveBeenCalledTimes(1);
  });

  it("checks a remembered listing against the file as it is now", async () => {
    // Listed as a small picture; the file has since grown past the cap.
    const contents = png(20 * MEBIBYTE);
    const { read, options } = server(contents);
    const view = await loadFileView(
      entry("photo.png", 70_000),
      options({ sizes: "remembered" }),
      never,
    );
    expect(view).toEqual({
      view: "tooLarge",
      canLoad: true,
      size: contents.length,
      dimensions: null,
    });
    expect(read.mock.calls.map(([length]) => length)).toEqual([70_000]);
    // Listed with the size it still has, but changed since.
    const later = server(encoder.encode("0123456789"), (LISTED_AT + 60) * 1000);
    const text = await loadFileView(
      entry("notes.txt", 10),
      later.options({ sizes: "remembered" }),
      never,
    );
    expect(text).toMatchObject({ view: "text", changed: true });
  });

  it("does not fetch a picture over the cap until asked to", async () => {
    const contents = png(20 * MEBIBYTE);
    const { read, options } = server(contents);
    const large = entry("scan.png", contents.length);
    expect(await loadFileView(large, options(), never)).toEqual({
      view: "tooLarge",
      canLoad: true,
      size: contents.length,
      dimensions: null,
    });
    expect(read).not.toHaveBeenCalled();
    const loaded = await loadFileView(large, options({ loadAnyway: true }), never);
    expect(loaded.view).toBe("image");
    expect(read).toHaveBeenCalledTimes(1);
  });

  it("goes by the file as it is now, and says when that is not how it was listed", async () => {
    // Listed as a short text; meanwhile a longer one.
    const contents = encoder.encode("grown\n".repeat(100));
    const { options } = server(contents);
    const view = await loadFileView(entry("notes.txt", 12), options(), never);
    expect(view).toMatchObject({ view: "text", changed: true, size: 600 });

    // The same size, written again later.
    const later = server(encoder.encode("abc\n"), (LISTED_AT + 500) * 1000);
    const rewritten = await loadFileView(entry("notes.txt", 4), later.options(), never);
    expect(rewritten).toMatchObject({ view: "text", changed: true });
    // Where listed times prove nothing, the size alone decides.
    const onFtp = await loadFileView(
      entry("notes.txt", 4),
      later.options({ exactTimes: false }),
      never,
    );
    expect(onFtp).toMatchObject({ view: "text", changed: false });
  });

  it("reads a link by what it leads to, whatever size the link is listed with", async () => {
    const contents = png(900_000);
    const { read, options } = server(contents);
    const link = entry("latest.png", 14, { kind: "symlink", linkTarget: "file" });
    const view = await loadFileView(link, options(), never);
    expect(view).toMatchObject({ view: "image", changed: false });
    expect(read.mock.calls.map(([length]) => length)).toEqual([caps.textBytes, 900_000]);
  });

  it("stops between reads when the cursor has moved on", async () => {
    const controller = new AbortController();
    const contents = png(300_000);
    const read = vi.fn(async (length: number): Promise<PreviewRead> => {
      controller.abort();
      return {
        stamp: { size: contents.length, modified: null },
        bytes: contents.subarray(0, length),
      };
    });
    const options: LoadOptions = { read, caps, loadAnyway: false, exactTimes: true };
    await expect(
      loadFileView(entry("photo.zip", contents.length), options, controller.signal),
    ).rejects.toBeInstanceOf(PreviewCancelled);
    expect(read).toHaveBeenCalledTimes(1);
  });

  it("gives up on a file that grows faster than it is read", async () => {
    let size = 1000;
    const read = vi.fn(async (length: number): Promise<PreviewRead> => {
      size += 5000;
      return { stamp: { size, modified: null }, bytes: png(Math.min(length, size)) };
    });
    const options: LoadOptions = { read, caps, loadAnyway: false, exactTimes: true };
    await expect(loadFileView(entry("growing.png", 1000), options, never)).rejects.toMatchObject({
      kind: "io",
    });
    expect(read).toHaveBeenCalledTimes(3);
  });

  it("passes a failed read on", async () => {
    const failure = { kind: "permissionDenied", message: "Permission denied" };
    const options: LoadOptions = {
      read: () => Promise.reject(failure),
      caps,
      loadAnyway: false,
      exactTimes: true,
    };
    await expect(loadFileView(entry("secret.key", 100), options, never)).rejects.toBe(failure);
  });
});

describe("viewWeight", () => {
  it("is what a view keeps in memory", () => {
    expect(viewWeight({ view: "image", format: "png", bytes: png(5000), changed: false })).toBe(
      5000,
    );
    expect(
      viewWeight({
        view: "text",
        text: "abcd",
        encoding: "utf8",
        partial: false,
        size: 4,
        changed: false,
      }),
    ).toBe(8);
    expect(viewWeight({ view: "empty" })).toBe(1);
    expect(viewWeight({ view: "tooLarge", canLoad: true, size: 9e9, dimensions: null })).toBe(1);
  });
});
