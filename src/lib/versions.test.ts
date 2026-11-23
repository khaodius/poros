import { describe, expect, it } from "vitest";
import type { DeletedEntry, DirListing, FileEntry, ObjectVersion } from "./types";
import {
  EDITOR_MAX_BYTES,
  afterDeleting,
  bucketOf,
  deletedAsEntry,
  deletedItems,
  deletionsIn,
  entriesStillDeleted,
  formatVersionTime,
  hasVersions,
  isBucket,
  isTextName,
  keepsVersions,
  mergeDeleted,
  restoreProgressText,
  shortTag,
  versionActions,
  versionFileName,
  versionStatus,
  versioningNote,
} from "./versions";

function entry(name: string, kind: FileEntry["kind"] = "file"): FileEntry {
  return {
    name,
    path: `/photos/2024/${name}`,
    kind,
    size: 10,
    modified: 1_700_000_000,
    permissions: null,
    owner: null,
    group: null,
    hidden: false,
  };
}

function deleted(name: string, isDir = false): DeletedEntry {
  return {
    name,
    path: `/photos/2024/${name}`,
    isDir,
    deleted: isDir ? null : 1_700_000_500,
    size: isDir ? 0 : 42,
  };
}

function version(versionId: string, overrides: Partial<ObjectVersion> = {}): ObjectVersion {
  return {
    versionId,
    kind: "version",
    latest: false,
    modified: 1_700_000_000,
    size: 100,
    ...overrides,
  };
}

function listing(entries: FileEntry[]): DirListing {
  return { path: "/photos/2024", parent: "/photos", entries };
}

describe("hasVersions", () => {
  it("is true for S3 and B2 only", () => {
    expect(hasVersions("s3")).toBe(true);
    expect(hasVersions("backblazeB2")).toBe(true);
    expect(hasVersions("sftp")).toBe(false);
    expect(hasVersions("webDav")).toBe(false);
    expect(hasVersions(undefined)).toBe(false);
  });
});

describe("bucketOf", () => {
  it("takes the first segment of a path", () => {
    expect(bucketOf("/photos/2024/a.jpg")).toBe("photos");
    expect(bucketOf("/photos")).toBe("photos");
    expect(bucketOf("photos/")).toBe("photos");
  });

  it("is null at the top, where buckets are listed", () => {
    expect(bucketOf("/")).toBeNull();
    expect(bucketOf("")).toBeNull();
  });
});

describe("isBucket", () => {
  it("is true for a bucket itself only", () => {
    expect(isBucket("/photos")).toBe(true);
    expect(isBucket("/photos/")).toBe(true);
    expect(isBucket("/photos/2024")).toBe(false);
    expect(isBucket("/")).toBe(false);
  });
});

describe("afterDeleting", () => {
  const current = version("v3", { latest: true });
  const marker = version("m1", { kind: "deleteMarker" });
  const older = version("v1");

  it("tells what the file shows once the current version goes, from the entry under it", () => {
    expect(afterDeleting(current, [current, older])).toBe(
      "The version before it becomes the current file.",
    );
    expect(afterDeleting(current, [current, marker, older])).toMatch(/show as deleted/);
    expect(afterDeleting(current, [current])).toMatch(/only version: the file will be gone/);
  });

  it("tells whether removing the marker on top brings the file back", () => {
    const top = version("m2", { kind: "deleteMarker", latest: true });
    expect(afterDeleting(top, [top, older])).toBe("The file comes back as its newest version.");
    expect(afterDeleting(top, [top, marker, older])).toMatch(/stays deleted/);
    expect(afterDeleting(top, [top])).toBe("Nothing of the file is left after it.");
  });

  it("says nothing for an older entry, whose going changes nothing", () => {
    expect(afterDeleting(older, [current, older])).toBeNull();
    expect(afterDeleting(marker, [current, marker, older])).toBeNull();
  });
});

describe("keepsVersions", () => {
  it("offers versions unless the bucket is known to keep none", () => {
    expect(keepsVersions("enabled")).toBe(true);
    expect(keepsVersions("suspended")).toBe(true);
    expect(keepsVersions("unknown")).toBe(true);
    expect(keepsVersions(undefined)).toBe(true);
    expect(keepsVersions("off")).toBe(false);
  });
});

describe("mergeDeleted", () => {
  it("adds deleted entries after the live ones and remembers which they are", () => {
    const live = listing([entry("kept.jpg"), entry("albums", "dir")]);
    const merged = mergeDeleted(live, [deleted("gone.jpg"), deleted("old", true)]);
    expect(merged.entries.map((item) => item.name)).toEqual([
      "kept.jpg",
      "albums",
      "gone.jpg",
      "old",
    ]);
    expect([...deletionsIn(merged).keys()]).toEqual(["/photos/2024/gone.jpg", "/photos/2024/old"]);
    expect(live.entries).toHaveLength(2);
  });

  it("leaves out a deleted entry whose path is listed again", () => {
    const merged = mergeDeleted(listing([entry("a.jpg")]), [deleted("a.jpg"), deleted("b.jpg")]);
    expect(merged.entries.map((item) => item.name)).toEqual(["a.jpg", "b.jpg"]);
    expect(deletionsIn(merged).has("/photos/2024/a.jpg")).toBe(false);
  });

  it("knows of no deletions in a listing it did not make", () => {
    expect(deletionsIn(listing([entry("a.jpg")])).size).toBe(0);
    expect(deletionsIn(null).size).toBe(0);
  });
});

describe("entriesStillDeleted", () => {
  it("keeps only what the listing on show still has as deleted", () => {
    const before = mergeDeleted(listing([entry("kept.jpg")]), [deleted("a.jpg"), deleted("b.jpg")]);
    const chosen = before.entries.filter((item) => deletionsIn(before).has(item.path));
    // a.jpg was written again since, and b.jpg was removed for good.
    const now = mergeDeleted(listing([entry("kept.jpg"), entry("a.jpg")]), []);
    expect(entriesStillDeleted(chosen, now)).toEqual([]);
    const still = mergeDeleted(listing([entry("kept.jpg")]), [deleted("b.jpg")]);
    expect(entriesStillDeleted(chosen, still).map((item) => item.name)).toEqual(["b.jpg"]);
    expect(entriesStillDeleted(chosen, null)).toEqual([]);
    // A listing without deleted files shows nothing as deleted.
    expect(entriesStillDeleted(chosen, listing([entry("a.jpg"), entry("b.jpg")]))).toEqual([]);
  });
});

describe("deletedAsEntry", () => {
  it("dates a deleted file by its deletion and keeps the size a restore brings back", () => {
    expect(deletedAsEntry(deleted("gone.jpg"))).toMatchObject({
      kind: "file",
      size: 42,
      modified: 1_700_000_500,
      hidden: false,
    });
  });

  it("makes folders and hidden names what they are", () => {
    expect(deletedAsEntry(deleted("old", true)).kind).toBe("dir");
    expect(deletedAsEntry(deleted(".env")).hidden).toBe(true);
  });
});

describe("deletedItems", () => {
  it("says which entries are folders", () => {
    expect(deletedItems([entry("a.jpg"), entry("old", "dir")])).toEqual([
      { path: "/photos/2024/a.jpg", isDir: false },
      { path: "/photos/2024/old", isDir: true },
    ]);
  });
});

describe("restoreProgressText", () => {
  it("counts what is restored while the total is still found", () => {
    expect(restoreProgressText({ files: 0 })).toBe("Finding what to restore...");
    expect(restoreProgressText({ files: 3 })).toBe("3 restored");
    expect(restoreProgressText({ files: 3, total: 10, counting: true })).toBe(
      "3 restored, 10 found so far",
    );
    expect(restoreProgressText({ files: 3, total: 10 })).toBe("3 of 10 restored");
  });
});

describe("versionStatus", () => {
  it("marks the current file, a delete on top and older markers", () => {
    expect(versionStatus(version("a", { latest: true }))).toBe("current");
    expect(versionStatus(version("a"))).toBeNull();
    expect(versionStatus(version("m", { kind: "deleteMarker", latest: true }))).toBe("deleted");
    expect(versionStatus(version("m", { kind: "deleteMarker" }))).toBe("marker");
  });
});

describe("isTextName", () => {
  it("tells text from names the editor has nothing to show for", () => {
    expect(isTextName("notes.txt")).toBe(true);
    expect(isTextName("Makefile")).toBe(true);
    expect(isTextName(".bashrc")).toBe(true);
    expect(isTextName("photo.JPG")).toBe(false);
    expect(isTextName("backup.tar.gz")).toBe(false);
  });
});

describe("versionActions", () => {
  const current = version("v3", { latest: true });
  const older = version("v2");
  const marker = version("m1", { kind: "deleteMarker" });
  const history = [current, marker, older];

  it("offers everything but restore for the current file", () => {
    expect([...versionActions(current, history, "notes.txt")].sort()).toEqual([
      "delete",
      "download",
      "open",
    ]);
  });

  it("offers restore and compare for an older text version", () => {
    expect([...versionActions(older, history, "notes.txt")].sort()).toEqual([
      "compare",
      "delete",
      "download",
      "open",
      "restore",
    ]);
  });

  it("only removes a delete marker", () => {
    expect([...versionActions(marker, history, "notes.txt")]).toEqual(["delete"]);
  });

  it("does not open or compare binary files or files past the editor's limit", () => {
    expect(versionActions(older, history, "photo.png").has("open")).toBe(false);
    expect(versionActions(older, history, "photo.png").has("compare")).toBe(false);
    const large = version("v1", { size: EDITOR_MAX_BYTES + 1 });
    expect(versionActions(large, [current, large], "notes.txt").has("open")).toBe(false);
  });

  it("compares with nothing when the file is deleted or too large now", () => {
    const deletedNow = [version("m2", { kind: "deleteMarker", latest: true }), older];
    expect(versionActions(older, deletedNow, "notes.txt").has("compare")).toBe(false);
    expect(versionActions(older, deletedNow, "notes.txt").has("restore")).toBe(true);
    const largeNow = [version("v9", { latest: true, size: EDITOR_MAX_BYTES + 1 }), older];
    expect(versionActions(older, largeNow, "notes.txt").has("compare")).toBe(false);
  });
});

describe("formatVersionTime", () => {
  it("shows seconds whatever the date format", () => {
    const seconds = new Date(2024, 2, 5, 9, 7, 3).getTime() / 1000;
    expect(formatVersionTime(seconds, "minutes")).toBe("2024-03-05 09:07:03");
    expect(formatVersionTime(seconds, "seconds")).toBe("2024-03-05 09:07:03");
    expect(formatVersionTime(seconds, "locale")).toMatch(/07.03/);
  });

  it("says when the time is unknown", () => {
    expect(formatVersionTime(null, "minutes")).toBe("Unknown time");
  });
});

describe("versionFileName", () => {
  const stamp = new Date(2024, 2, 5, 9, 7, 3).getTime() / 1000;

  it("puts an older version's time before the extension", () => {
    expect(versionFileName("report.pdf", version("v1", { modified: stamp }))).toBe(
      "report (2024-03-05 09-07-03).pdf",
    );
    expect(versionFileName("Makefile", version("v1", { modified: stamp }))).toBe(
      "Makefile (2024-03-05 09-07-03)",
    );
    expect(versionFileName(".env", version("v1", { modified: stamp }))).toBe(
      ".env (2024-03-05 09-07-03)",
    );
  });

  it("keeps the name of the current version or one of unknown time", () => {
    expect(versionFileName("report.pdf", version("v2", { latest: true }))).toBe("report.pdf");
    expect(versionFileName("report.pdf", version("v1", { modified: null }))).toBe("report.pdf");
  });
});

describe("shortTag", () => {
  it("shortens long ETags only", () => {
    expect(shortTag("0123456789abcdef")).toBe("01234567...");
    expect(shortTag("abc")).toBe("abc");
  });
});

describe("versioningNote", () => {
  it("explains suspended and unknown versioning only", () => {
    expect(versioningNote("suspended")).toMatch(/suspended/);
    expect(versioningNote("unknown")).toMatch(/may not see/);
    expect(versioningNote("enabled")).toBeNull();
    expect(versioningNote(undefined)).toBeNull();
  });
});
