import { describe, expect, it } from "vitest";
import sharedCases from "../../src-tauri/tests/fixtures/folder_compare_cases.json?raw";
import {
  compareEntry,
  compareListings,
  compareRules,
  decide,
  deepPrefix,
  describeSummary,
  ignoresCase,
  legendCounts,
  namesIn,
  pathBelow,
  rowMarks,
  sameEntries,
  sameMoment,
  sizeOnlyIn,
  type CompareEntry,
  type CompareRules,
  type DeepComparison,
  type Outcome,
} from "./folderCompare";
import { DEFAULT_SETTINGS } from "./settings";
import type { FileEntry, FolderSummary } from "./types";

const RULES: CompareRules = {
  sizeOnly: false,
  toleranceSecs: 2,
  ignoreHourOffsets: false,
  includeHidden: true,
  ignoreCase: false,
  leftCoarse: false,
  rightCoarse: false,
};

function file(name: string, size: number, modified: number | null, hidden = false): CompareEntry {
  return { name, shape: "file", size, modified, hidden };
}

function folder(name: string, hidden = false): CompareEntry {
  return { name, shape: "folder", size: 0, modified: 0, hidden };
}

/** One row of the table the Rust rule is tested against too (`sync/folder_compare.rs`). */
interface SharedCase {
  name: string;
  sizeOnly?: boolean;
  tolerance?: number;
  ignoreHourOffsets?: boolean;
  leftCoarse?: boolean;
  rightCoarse?: boolean;
  left: SharedThing | null;
  right: SharedThing | null;
  expect: Outcome;
}

interface SharedThing {
  folder?: boolean;
  size?: number;
  modified: number | null;
}

function shared(thing: SharedThing | null) {
  if (!thing) return undefined;
  return {
    shape: thing.folder ? ("folder" as const) : ("file" as const),
    size: thing.size ?? 0,
    modified: thing.modified,
  };
}

describe("the rule a synchronization decides by", () => {
  const cases = JSON.parse(sharedCases) as SharedCase[];

  it("has the table the Rust rule is checked against", () => {
    expect(cases.length).toBeGreaterThan(30);
    expect(new Set(cases.map((entry) => entry.name)).size).toBe(cases.length);
  });

  it.each(cases.map((entry) => [entry.name, entry] as const))("%s", (_name, entry) => {
    const rules: CompareRules = {
      ...RULES,
      sizeOnly: entry.sizeOnly ?? false,
      toleranceSecs: entry.tolerance ?? 2,
      ignoreHourOffsets: entry.ignoreHourOffsets ?? false,
      leftCoarse: entry.leftCoarse ?? false,
      rightCoarse: entry.rightCoarse ?? false,
    };
    expect(decide(shared(entry.left), shared(entry.right), rules)).toBe(entry.expect);
  });

  it("decides nothing about what a synchronization passes over", () => {
    const special = { shape: "other" as const, size: 0, modified: 5 };
    const regular = { shape: "file" as const, size: 3, modified: 5 };
    expect(decide(special, regular, RULES)).toBe("passedOver");
    expect(decide(regular, special, RULES)).toBe("passedOver");
    expect(decide(special, undefined, RULES)).toBe("passedOver");
    expect(decide(undefined, special, RULES)).toBe("passedOver");
  });

  it("counts times a whole number of hours apart as one moment only when asked", () => {
    const zones = { ...RULES, ignoreHourOffsets: true };
    expect(sameMoment(7200, 0, RULES)).toBe(false);
    expect(sameMoment(7200, 0, zones)).toBe(true);
    expect(sameMoment(0, 7201, zones)).toBe(true);
    expect(sameMoment(0, 7203, zones)).toBe(false);
    // Times before 1970 are negative, as the Rust rule's Euclidean division allows for.
    expect(sameMoment(-3600, 0, zones)).toBe(true);
    expect(sameMoment(-1800, 0, zones)).toBe(false);
  });
});

describe("comparing two listings", () => {
  const left = [
    folder("docs"),
    folder("only-left"),
    file("same.txt", 10, 100),
    file("edited-here.txt", 12, 500),
    file("edited-there.txt", 10, 100),
    file("tie.txt", 5, 100),
    file("new-here.txt", 1, 1),
    file("kind", 1, 1),
  ];
  const right = [
    folder("docs"),
    folder("kind"),
    file("same.txt", 10, 101),
    file("edited-here.txt", 10, 100),
    file("edited-there.txt", 14, 900),
    file("tie.txt", 6, 101),
    file("new-there.txt", 1, 1),
  ];

  it("gives each side its own view of every name", () => {
    const found = compareListings(left, right, RULES);
    expect(Object.fromEntries(found.left)).toEqual({
      docs: "both",
      "only-left": "onlyHere",
      "same.txt": "identical",
      "edited-here.txt": "newer",
      "edited-there.txt": "older",
      "tie.txt": "different",
      "new-here.txt": "onlyHere",
      kind: "different",
    });
    expect(Object.fromEntries(found.right)).toEqual({
      docs: "both",
      kind: "different",
      "same.txt": "identical",
      "edited-here.txt": "older",
      "edited-there.txt": "newer",
      "tie.txt": "different",
      "new-there.txt": "onlyHere",
    });
  });

  it("is the same comparison whichever pane is on the left", () => {
    const one = compareListings(left, right, RULES);
    const other = compareListings(right, left, RULES);
    expect(other.right).toEqual(one.left);
    expect(other.left).toEqual(one.right);
  });

  it("leaves hidden entries out unless they count", () => {
    const here = [file(".env", 3, 100, true), file("a.txt", 1, 1)];
    const there = [file("a.txt", 1, 1)];
    const counted = compareListings(here, there, RULES);
    expect(counted.left.get(".env")).toBe("onlyHere");
    const left_out = compareListings(here, there, { ...RULES, includeHidden: false });
    expect(left_out.left.has(".env")).toBe(false);
    expect(left_out.left.get("a.txt")).toBe("identical");
    // Hidden on one side only: the side that hides it is not compared, so the other has it alone.
    const shown = compareListings([file("notes", 1, 1)], [file("notes", 1, 1, true)], {
      ...RULES,
      includeHidden: false,
    });
    expect(shown.left.get("notes")).toBe("onlyHere");
    expect(shown.right.size).toBe(0);
  });

  it("matches names whatever their case when either side ignores it", () => {
    const here = [file("Read Me.TXT", 3, 100), folder("Docs")];
    const there = [file("read me.txt", 3, 100), folder("docs")];
    const exact = compareListings(here, there, RULES);
    expect(exact.left.get("Read Me.TXT")).toBe("onlyHere");
    const folded = compareListings(here, there, { ...RULES, ignoreCase: true });
    expect(folded.left.get("Read Me.TXT")).toBe("identical");
    expect(folded.right.get("read me.txt")).toBe("identical");
    expect(folded.left.get("Docs")).toBe("both");
  });

  it("does not compare names one side holds in two cases when case is ignored", () => {
    const here = [file("a.txt", 5, 900), file("A.txt", 5, 100), folder("Docs"), folder("docs")];
    const there = [file("a.txt", 5, 900), folder("DOCS"), file("b.txt", 1, 1)];
    const found = compareListings(here, there, { ...RULES, ignoreCase: true });
    // Which of the two the other side's name stands for cannot be told, so neither gets a mark,
    // as the synchronization passes all of them over.
    expect(Object.fromEntries(found.left)).toEqual({
      "a.txt": "passedOver",
      "A.txt": "passedOver",
      Docs: "passedOver",
      docs: "passedOver",
    });
    expect(Object.fromEntries(found.right)).toEqual({
      "a.txt": "passedOver",
      DOCS: "passedOver",
      "b.txt": "onlyHere",
    });
    expect(rowMarks(found.left, "left").size).toBe(0);
    // Where case tells names apart, the two are separate names.
    const exact = compareListings(here, there, RULES);
    expect(exact.left.get("a.txt")).toBe("identical");
    expect(exact.left.get("A.txt")).toBe("onlyHere");
    expect(exact.right.get("DOCS")).toBe("onlyHere");
  });

  it("compares a hundred thousand entries in one pass each", () => {
    const many = (offset: number) =>
      Array.from({ length: 100_000 }, (_unused, index) =>
        file(`file-${index + offset}.dat`, index, 1_700_000_000 + (index % 7)),
      );
    const started = performance.now();
    const found = compareListings(many(0), many(50_000), RULES);
    expect(found.left.size).toBe(100_000);
    expect(found.left.get("file-0.dat")).toBe("onlyHere");
    expect(found.right.get("file-149999.dat")).toBe("onlyHere");
    // Generous, so a busy machine passes; a comparison per pair of rows would take minutes.
    expect(performance.now() - started).toBeLessThan(5000);
  });
});

describe("what the page knows of each side", () => {
  const entry = (overrides: Partial<FileEntry>): FileEntry => ({
    name: "item",
    path: "/srv/item",
    kind: "file",
    size: 7,
    modified: 60,
    permissions: null,
    owner: null,
    group: null,
    hidden: false,
    ...overrides,
  });

  it("reads links as what they lead to", () => {
    expect(compareEntry(entry({})).shape).toBe("file");
    expect(compareEntry(entry({ kind: "dir" })).shape).toBe("folder");
    expect(compareEntry(entry({ kind: "symlink", linkTarget: "dir" })).shape).toBe("folder");
    expect(compareEntry(entry({ kind: "symlink", linkTarget: "file" })).shape).toBe("file");
    expect(compareEntry(entry({ kind: "symlink", linkTarget: "broken" })).shape).toBe("other");
    expect(compareEntry(entry({ kind: "symlink" })).shape).toBe("other");
    expect(compareEntry(entry({ kind: "other" })).shape).toBe("other");
  });

  it("knows where case does not tell names apart", () => {
    expect(ignoresCase("windows", undefined)).toBe(true);
    expect(ignoresCase("posix", undefined)).toBe(false);
    expect(ignoresCase("posix", "oneDrive")).toBe(true);
    expect(ignoresCase("posix", "box")).toBe(true);
    expect(ignoresCase("posix", "sftp")).toBe(false);
  });

  it("tells a listing that changed from one that only loaded again", () => {
    const listed = [file("a", 1, 1), folder("b")];
    expect(sameEntries(listed, [file("a", 1, 1), folder("b")])).toBe(true);
    expect(sameEntries(listed, [file("a", 2, 1), folder("b")])).toBe(false);
    expect(sameEntries(listed, [file("a", 1, 1)])).toBe(false);
    expect(sameEntries(listed, [folder("b"), file("a", 1, 1)])).toBe(false);
  });

  it("takes its rules from the bar and from Settings > Sync", () => {
    const options = DEFAULT_SETTINGS.interface.folderCompare;
    const sides = { ignoreCase: false, leftCoarse: false, rightCoarse: true };
    const rules = compareRules(options, DEFAULT_SETTINGS.sync.options, sides);
    expect(rules).toEqual({ ...RULES, rightCoarse: true });
    expect(sizeOnlyIn(options, "sizeOnly")).toBe(true);
    expect(sizeOnlyIn(options, "checksum")).toBe(false);
    expect(sizeOnlyIn({ ...options, sizeRule: "sizeAndTime" }, "sizeOnly")).toBe(false);
    expect(sizeOnlyIn({ ...options, sizeRule: "sizeOnly" }, "sizeAndTime")).toBe(true);
    const wide = compareRules(options, { compare: "sizeAndTime", timeToleranceSecs: 1e9 }, sides);
    expect(wide.toleranceSecs).toBe(86400);
    const broken = compareRules(options, { compare: "sizeAndTime", timeToleranceSecs: NaN }, sides);
    expect(broken.toleranceSecs).toBe(0);
  });
});

describe("folders compared below the ones on show", () => {
  const summary = (path: string, overrides: Partial<FolderSummary> = {}): FolderSummary => ({
    path,
    identical: 0,
    newerLeft: 0,
    newerRight: 0,
    onlyLeft: 0,
    onlyRight: 0,
    different: 0,
    notCompared: 0,
    ...overrides,
  });
  const deepOf = (summaries: FolderSummary[], ignoreCase = false): DeepComparison => ({
    leftRoot: "/home/me/site",
    rightRoot: "/var/www",
    ignoreCase,
    folders: new Map(summaries.map((entry) => [entry.path, entry])),
    truncated: false,
  });

  it("finds a path below a root in either path style", () => {
    expect(pathBelow("/var/www", "/var/www/", "posix")).toBe("");
    expect(pathBelow("/var/www/css/img", "/var/www", "posix")).toBe("css/img");
    expect(pathBelow("/var/wwwroot", "/var/www", "posix")).toBeNull();
    expect(pathBelow("/etc", "/", "posix")).toBe("etc");
    expect(pathBelow("C:\\Site\\Css", "c:\\site", "windows")).toBe("css");
    expect(pathBelow("C:\\Site", "C:\\", "windows")).toBe("site");
    expect(pathBelow("D:\\Site", "C:\\", "windows")).toBeNull();
  });

  it("knows when two panes show the same folder of the comparison", () => {
    const deep = deepOf([]);
    const at = (path: string) => ({ path, pathStyle: "posix" as const });
    expect(deepPrefix(deep, at("/home/me/site"), at("/var/www"))).toBe("");
    expect(deepPrefix(deep, at("/home/me/site/css"), at("/var/www/css"))).toBe("css");
    expect(deepPrefix(deep, at("/home/me/site/css"), at("/var/www/js"))).toBeNull();
    expect(deepPrefix(deep, at("/home/me/site/css"), at("/var/www"))).toBeNull();
    expect(deepPrefix(deep, at("/home/me"), at("/var/www"))).toBeNull();
    expect(deepPrefix(deep, at("/home/me/site/CSS"), at("/var/www/css"))).toBeNull();
    const folded = deepOf([], true);
    expect(deepPrefix(folded, at("/home/me/site/CSS"), at("/var/www/css"))).toBe("css");
  });

  it("marks folders by what was found inside them", () => {
    const states = new Map([
      ["same", "both"],
      ["mixed", "both"],
      ["unread", "both"],
      ["unknown", "both"],
      ["file.txt", "newer"],
    ] as const);
    const deep = deepOf([
      summary("css/same", { identical: 4 }),
      summary("css/mixed", { identical: 2, newerLeft: 1, onlyRight: 3, different: 1 }),
      summary("css/unread", { identical: 1, notCompared: 2 }),
    ]);
    const left = rowMarks(states, "left", { comparison: deep, prefix: "css" });
    expect(left.get("same")).toEqual({ state: "identical", title: "Identical inside: 4 files" });
    expect(left.get("mixed")).toEqual({
      state: "differsInside",
      title: "Differs inside: 1 newer here, 3 only in the other pane, 1 different, 2 identical",
    });
    expect(left.get("unread")?.state).toBe("both");
    expect(left.get("unread")?.title).toContain("2 not compared");
    expect(left.get("unknown")?.title).toBe("On both sides. The contents are not compared.");
    expect(left.get("file.txt")?.state).toBe("newer");
    const right = rowMarks(states, "right", { comparison: deep, prefix: "css" });
    expect(right.get("mixed")?.title).toBe(
      "Differs inside: 1 older here, 3 only here, 1 different, 2 identical",
    );
    // Without a deep comparison, and at another folder of it, folders stay unjudged.
    expect(rowMarks(states, "left").get("same")?.state).toBe("both");
    expect(rowMarks(states, "left", { comparison: deep, prefix: "" }).get("same")?.state).toBe(
      "both",
    );
  });

  it("finds folders whatever their case when case is ignored", () => {
    const states = new Map([["Photos", "both"]] as const);
    const deep = deepOf([summary("photos", { identical: 1 })], true);
    const marks = rowMarks(states, "left", { comparison: deep, prefix: "" });
    expect(marks.get("Photos")).toEqual({ state: "identical", title: "Identical inside: 1 file" });
  });

  it("says in words what a folder holds", () => {
    expect(describeSummary(summary("a", { identical: 1200, onlyLeft: 1 }), "right")).toBe(
      `1 only in the other pane, ${(1200).toLocaleString()} identical`,
    );
    expect(
      rowMarks(new Map([["a", "both"]] as const), "left", {
        comparison: deepOf([summary("a")]),
        prefix: "",
      }).get("a")?.title,
    ).toBe("Identical inside: no files on either side");
  });

  it("counts the rows a pane can show, and picks rows by state", () => {
    const marks = rowMarks(
      new Map([
        ["a", "newer"],
        ["b", "onlyHere"],
        ["c", "identical"],
        ["d", "both"],
        [".e", "onlyHere"],
        ["f", "passedOver"],
      ] as const),
      "left",
    );
    expect(marks.has("f")).toBe(false);
    const shown = ["a", "b", "c", "d", "f", "unmarked"].map((name) => ({ name }));
    expect(legendCounts(shown, marks)).toEqual({
      newer: 1,
      older: 0,
      onlyHere: 1,
      different: 0,
      identical: 1,
    });
    expect([...namesIn(marks, ["newer", "onlyHere"])]).toEqual(["a", "b", ".e"]);
    expect([...namesIn(marks, ["different"])]).toEqual([]);
  });

  it("counts and picks a folder that differs inside apart from different rows", () => {
    const deep = deepOf([
      summary("mixed", { identical: 1, newerLeft: 1 }),
      summary("same", { identical: 2 }),
    ]);
    const marks = rowMarks(
      new Map([
        ["mixed", "both"],
        ["same", "both"],
        ["kind", "different"],
        ["file.txt", "different"],
      ] as const),
      "left",
      { comparison: deep, prefix: "" },
    );
    expect(marks.get("mixed")).toEqual({
      state: "differsInside",
      title: "Differs inside: 1 newer here, 1 identical",
    });
    const shown = ["mixed", "same", "kind", "file.txt"].map((name) => ({ name }));
    expect(legendCounts(shown, marks)).toEqual({
      newer: 0,
      older: 0,
      onlyHere: 0,
      different: 2,
      identical: 1,
    });
    expect([...namesIn(marks, ["different"])]).toEqual(["kind", "file.txt"]);
    expect([...namesIn(marks, ["differsInside"])]).toEqual(["mixed"]);
  });
});
