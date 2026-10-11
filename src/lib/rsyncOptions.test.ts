import { describe, expect, it } from "vitest";
import {
  DEFAULT_SYNC_OPTIONS,
  OPTION_GROUPS,
  changedCount,
  commandPreview,
  formatSizeArgument,
  legacySyncOptions,
  parseSize,
  sanitizeSyncOptions,
  unavailable,
  withOption,
} from "./rsyncOptions";

describe("sizes", () => {
  it("reads sizes as rsync does", () => {
    expect(parseSize("")).toBeNull();
    expect(parseSize("  ")).toBeNull();
    expect(parseSize("100")).toBe(100);
    expect(parseSize("100b")).toBe(100);
    expect(parseSize("500K")).toBe(500 * 1024);
    expect(parseSize("1.5m")).toBe(1.5 * 1024 * 1024);
    expect(parseSize("2G")).toBe(2 * 1024 ** 3);
    expect(parseSize("10KB")).toBe(10_000);
    expect(parseSize("10KiB")).toBe(10 * 1024);
    expect(parseSize("10 M")).toBe(10 * 1024 * 1024);
  });

  it("turns down what rsync would", () => {
    expect(parseSize("ten")).toBeUndefined();
    expect(parseSize("-5")).toBeUndefined();
    expect(parseSize("5Ki")).toBeUndefined();
    expect(parseSize("5bb")).toBeUndefined();
    expect(parseSize("5X")).toBeUndefined();
  });

  it("writes sizes in the largest whole unit", () => {
    expect(formatSizeArgument(null)).toBe("");
    expect(formatSizeArgument(0)).toBe("0B");
    expect(formatSizeArgument(1000)).toBe("1000B");
    expect(formatSizeArgument(1024)).toBe("1K");
    expect(formatSizeArgument(3 * 1024 ** 3)).toBe("3G");
    for (const size of [1, 1536, 1024 ** 2, 7 * 1024 ** 4]) {
      expect(parseSize(formatSizeArgument(size))).toBe(size);
    }
  });
});

describe("stored options", () => {
  it("keeps values of the right kind and defaults the rest", () => {
    const options = sanitizeSyncOptions({
      compare: "checksum",
      append: "sideways",
      excludes: ["*.tmp", 4, "build/"],
      minSize: -1,
      maxSize: 2048.4,
      blockSize: "big",
      bwlimitKib: -50,
      timeToleranceSecs: 10 ** 9,
      perms: "yes",
      owner: true,
      chmod: "D755,F644",
      unknown: true,
    });
    expect(options.compare).toBe("checksum");
    expect(options.append).toBe("off");
    expect(options.excludes).toEqual(["*.tmp", "build/"]);
    expect(options.minSize).toBeNull();
    expect(options.maxSize).toBe(2048);
    expect(options.blockSize).toBeNull();
    expect(options.bwlimitKib).toBe(0);
    expect(options.timeToleranceSecs).toBe(24 * 60 * 60);
    expect(options.perms).toBe(false);
    expect(options.owner).toBe(true);
    expect(options.chmod).toBe("D755,F644");
    expect("unknown" in options).toBe(false);
    expect(sanitizeSyncOptions(null)).toEqual(DEFAULT_SYNC_OPTIONS);
  });

  it("reads the options saved before the full set", () => {
    const options = legacySyncOptions({
      direction: "download",
      compare: "sizeOnly",
      deleteExtraneous: true,
      excludes: "*.log\n\nnode_modules/\r\n",
    });
    expect(options.compare).toBe("sizeOnly");
    expect(options.deleteExtraneous).toBe(true);
    expect(options.excludes).toEqual(["*.log", "node_modules/"]);
    expect(options.recursive).toBe(true);
  });
});

describe("editing", () => {
  it("counts the options of a group that differ from the defaults", () => {
    const deletion = OPTION_GROUPS.find((group) => group.id === "delete")!;
    expect(changedCount(deletion, DEFAULT_SYNC_OPTIONS)).toBe(0);
    const changed = { ...DEFAULT_SYNC_OPTIONS, deleteExtraneous: true, maxDelete: 10 };
    expect(changedCount(deletion, changed)).toBe(2);
  });

  it("gives every option exactly one field", () => {
    const keys = OPTION_GROUPS.flatMap((group) => group.fields.map((field) => field.key));
    expect(new Set(keys).size).toBe(keys.length);
    expect([...keys].sort()).toEqual(Object.keys(DEFAULT_SYNC_OPTIONS).sort());
  });

  it("compares by time both ways", () => {
    const always = { ...DEFAULT_SYNC_OPTIONS, compare: "always" as const };
    expect(withOption(always, "perms", true, "both").compare).toBe("sizeAndTime");
    expect(withOption(always, "perms", true, "upload").compare).toBe("always");
    expect(unavailable("deleteExtraneous", "both")).toBe(true);
    expect(unavailable("deleteExtraneous", "download")).toBe(false);
    expect(unavailable("compare", "both")).toBe(false);
  });

  it("shows the command with the folders in rsync's order", () => {
    expect(commandPreview("-rt --delete", "upload", "/home/me/site", "host:/srv/site")).toBe(
      "rsync -rt --delete /home/me/site/ host:/srv/site/",
    );
    expect(commandPreview("-rt", "download", "C:\\Users\\me\\My Site", "host:/srv/")).toBe(
      "rsync -rt host:/srv/ 'C:\\Users\\me\\My Site\\'",
    );
    expect(commandPreview("", "upload", "", "")).toBe("rsync LOCAL/ SERVER:PATH/");
  });
});
