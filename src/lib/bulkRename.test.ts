import { describe, expect, it } from "vitest";
import {
  DEFAULT_RULES,
  FIND_HISTORY_LIMIT,
  applyLetterCase,
  caseProbe,
  changedRange,
  compileFind,
  counterValues,
  findHistoryLabel,
  folderName,
  formatCounter,
  formatDateToken,
  formatTimeToken,
  joinName,
  nameKey,
  nameProblem,
  newNames,
  planRenames,
  previewRenames,
  problemSummary,
  rememberFind,
  renameOutcomes,
  renamesOf,
  sanitizeFindHistory,
  splitName,
  stepsOf,
  targetTraits,
  type FindRule,
  type NameTarget,
  type PlannedRename,
  type RenameGroup,
  type RenameItem,
  type RenameRules,
  type RenameSelection,
} from "./bulkRename";

const POSIX: NameTarget = {
  forbidden: "/",
  windowsNames: false,
  caseInsensitive: false,
  normalizes: false,
  maxLength: 255,
  lengthUnit: "bytes",
};
const WINDOWS: NameTarget = {
  forbidden: '<>:"/\\|?*',
  windowsNames: true,
  caseInsensitive: true,
  normalizes: false,
  maxLength: 255,
  lengthUnit: "characters",
};
const VAULT: NameTarget = { ...POSIX, normalizes: true };

/** A time with the same reading in every time zone the tests may run in. */
const MODIFIED = new Date(2024, 2, 9, 7, 5, 3).getTime() / 1000;

function file(name: string, modified: number | null = MODIFIED): RenameItem {
  return { name, isFolder: false, modified };
}

function folder(name: string): RenameItem {
  return { name, isFolder: true, modified: MODIFIED };
}

function selectionOf(
  items: (RenameItem | string)[],
  others: string[] = [],
  parent = "holiday",
): RenameSelection {
  return {
    items: items.map((item) => (typeof item === "string" ? file(item) : item)),
    parent,
    others,
  };
}

type RuleChanges = { [Key in keyof RenameRules]?: Partial<RenameRules[Key]> | RenameRules[Key] };

/** The default rules with some parts changed, and find and replace doing nothing unless set. */
function rulesWith(changes: RuleChanges = {}): RenameRules {
  const rules = structuredClone(DEFAULT_RULES);
  for (const [key, change] of Object.entries(changes) as [keyof RenameRules, unknown][]) {
    if (typeof change === "string") Object.assign(rules, { [key]: change });
    else Object.assign(rules[key] as object, change);
  }
  return rules;
}

function find(pattern: string, replacement: string, more: Partial<FindRule> = {}): RuleChanges {
  return { find: { pattern, replacement, ...more } };
}

function regex(pattern: string, replacement: string, more: Partial<FindRule> = {}): RuleChanges {
  return find(pattern, replacement, { mode: "regex", caseSensitive: true, ...more });
}

function renamed(items: (RenameItem | string)[], changes: RuleChanges, parent?: string): string[] {
  return newNames(selectionOf(items, [], parent), rulesWith(changes)).names;
}

describe("splitName", () => {
  it("takes what follows the last dot as the extension", () => {
    expect(splitName("photo.jpg", false)).toEqual({ stem: "photo", extension: "jpg" });
    expect(splitName("archive.tar.gz", false)).toEqual({ stem: "archive.tar", extension: "gz" });
    expect(splitName("a.b.c.d", false)).toEqual({ stem: "a.b.c", extension: "d" });
  });

  it("finds no extension in dotfiles, bare names and names ending in a dot", () => {
    expect(splitName(".bashrc", false)).toEqual({ stem: ".bashrc", extension: null });
    expect(splitName("Makefile", false)).toEqual({ stem: "Makefile", extension: null });
    expect(splitName("draft.", false)).toEqual({ stem: "draft.", extension: null });
    expect(splitName(".config.json", false)).toEqual({ stem: ".config", extension: "json" });
  });

  it("gives folders no extension", () => {
    expect(splitName("photos.2024", true)).toEqual({ stem: "photos.2024", extension: null });
  });

  it("joins back to the same name", () => {
    for (const name of ["photo.jpg", "archive.tar.gz", ".bashrc", "Makefile", "draft.", "a.b"]) {
      expect(joinName(splitName(name, false))).toBe(name);
    }
    expect(joinName({ stem: "notes", extension: "" })).toBe("notes");
  });
});

describe("find and replace", () => {
  it("replaces plain text in the name and leaves the extension alone", () => {
    expect(renamed(["IMG_0001.img", "IMG_0002.IMG"], find("img", "photo"))).toEqual([
      "photo_0001.img",
      "photo_0002.IMG",
    ]);
  });

  it("takes plain text literally, in the pattern and in the replacement", () => {
    expect(renamed(["a.b (1).txt", "axb (1).txt"], find("a.b (1)", "$1 $& c"))).toEqual([
      "$1 $& c.txt",
      "axb (1).txt",
    ]);
  });

  it("matches case only when asked to", () => {
    expect(renamed(["Report.txt"], find("report", "summary"))).toEqual(["summary.txt"]);
    expect(renamed(["Report.txt"], find("report", "summary", { caseSensitive: true }))).toEqual([
      "Report.txt",
    ]);
    expect(renamed(["ÉTÉ.txt"], find("été", "summer"))).toEqual(["summer.txt"]);
  });

  it("replaces the first match or all of them", () => {
    expect(renamed(["a-b-c.txt"], find("-", "_"))).toEqual(["a_b_c.txt"]);
    expect(renamed(["a-b-c.txt"], find("-", "_", { allMatches: false }))).toEqual(["a_b-c.txt"]);
    expect(renamed(["a1b22.txt"], regex("\\d+", "#", { allMatches: false }))).toEqual([
      "a#b22.txt",
    ]);
  });

  it("works on the name, the extension, or the whole name", () => {
    const items = ["jpeg.photo.jpeg", "jpeg"];
    expect(renamed(items, find("jpeg", "jpg", { scope: "name" }))).toEqual([
      "jpg.photo.jpeg",
      "jpg",
    ]);
    expect(renamed(items, find("jpeg", "jpg", { scope: "extension" }))).toEqual([
      "jpeg.photo.jpg",
      "jpeg",
    ]);
    expect(renamed(items, find("jpeg", "jpg", { scope: "both" }))).toEqual([
      "jpg.photo.jpg",
      "jpg",
    ]);
  });

  it("lets a pattern for the whole name reach across the dot", () => {
    expect(
      renamed(["scan.final.jpeg"], regex("\\.final\\.jpeg$", ".jpg", { scope: "both" })),
    ).toEqual(["scan.jpg"]);
  });

  it("treats a folder's whole name as its name", () => {
    expect(renamed([folder("v1.2"), file("v1.2")], find(".2", ".3"))).toEqual(["v1.3", "v1.2"]);
  });

  it("does nothing while the pattern is empty or the rule is off", () => {
    expect(renamed(["a.txt"], find("", "x"))).toEqual(["a.txt"]);
    expect(renamed(["a.txt"], find("a", "x", { enabled: false }))).toEqual(["a.txt"]);
  });

  describe("regular expressions", () => {
    it("fills in numbered and named groups", () => {
      expect(renamed(["2024-03-09 trip.jpg"], regex("(\\d+)-(\\d+)-(\\d+)", "$3.$2.$1"))).toEqual([
        "09.03.2024 trip.jpg",
      ]);
      expect(
        renamed(["IMG_0042.jpg"], regex("IMG_(?<number>\\d+)", "photo-$<number>-[$&]")),
      ).toEqual(["photo-0042-[IMG_0042].jpg"]);
    });

    it("writes a dollar sign with $$", () => {
      expect(renamed(["price.txt"], regex("price", "$$5"))).toEqual(["$5.txt"]);
    });

    it("handles matches of nothing", () => {
      expect(renamed(["abc.txt"], regex("^", "new-"))).toEqual(["new-abc.txt"]);
      expect(renamed(["abc.txt"], regex("$", "-old"))).toEqual(["abc-old.txt"]);
      expect(renamed(["abc.txt"], regex("x*", "-"))).toEqual(["-a-b-c-.txt"]);
      expect(renamed(["abc.txt"], regex("x*", "-", { allMatches: false }))).toEqual(["-abc.txt"]);
      expect(renamed(["abc.txt"], regex("(?=b)", "_"))).toEqual(["a_bc.txt"]);
    });

    it("supports lookbehind", () => {
      expect(renamed(["v1 v2 x3.txt"], regex("(?<=v)\\d", "#"))).toEqual(["v# v# x3.txt"]);
      expect(renamed(["v1 x3.txt"], regex("(?<!v)\\d", "#"))).toEqual(["v1 x#.txt"]);
    });

    it("matches whole characters, never half an emoji", () => {
      expect(renamed(["a😀b.txt"], regex(".", "[$&]"))).toEqual(["[a][😀][b].txt"]);
      expect(renamed(["😀😀.txt"], regex("x*", "-"))).toEqual(["-😀-😀-.txt"]);
      expect(renamed(["żółw.txt"], regex("\\p{L}", "x"))).toEqual(["xxxx.txt"]);
    });

    it("ignores case when asked to, also outside ASCII", () => {
      expect(renamed(["ÄRGER.txt"], regex("ärger", "ok", { caseSensitive: false }))).toEqual([
        "ok.txt",
      ]);
    });

    it("reports an invalid pattern in the engine's words and renames nothing", () => {
      for (const pattern of ["(", "[a-", "a{2,1}", "(?<name>x)(?<name>y)", "\\"]) {
        const check = compileFind({ ...rulesWith(regex(pattern, "x")).find });
        expect(check.find, pattern).toBeNull();
        expect(check.error, pattern).toBeTruthy();
        expect(check.error, pattern).not.toContain("Invalid regular expression");
      }
      const result = newNames(selectionOf(["a(.txt"]), rulesWith(regex("(", "x")));
      expect(result.names).toEqual(["a(.txt"]);
      expect(result.findError).toMatch(/group|parenthes/i);
    });

    it("finds nothing wrong with a valid pattern, or with plain text that would not compile", () => {
      expect(compileFind(rulesWith(regex("(a|b)+", "x")).find).error).toBeNull();
      expect(compileFind(rulesWith(find("(", "x")).find).error).toBeNull();
      expect(renamed(["a(1.txt"], find("(", "["))).toEqual(["a[1.txt"]);
    });
  });
});

describe("template", () => {
  const template = (pattern: string, more: Partial<RenameRules["template"]> = {}): RuleChanges => ({
    template: { enabled: true, pattern, ...more },
  });

  it("builds the name from tokens and keeps the extension", () => {
    expect(renamed(["IMG_1.jpg", "IMG_2.png"], template("{parent}-{name}"), "holiday")).toEqual([
      "holiday-IMG_1.jpg",
      "holiday-IMG_2.png",
    ]);
  });

  it("places the extension itself when the template names it", () => {
    expect(renamed(["a.jpg", "b"], template("{name}.backup.{ext}"))).toEqual([
      "a.backup.jpg",
      "b.backup",
    ]);
    expect(renamed(["a.jpg", "b"], template("{ext}_{name}"))).toEqual(["jpg_a", "_b"]);
    expect(renamed([folder("set.one")], template("{name}.{ext}"))).toEqual(["set.one"]);
  });

  it("numbers items with the counter's settings", () => {
    const changes: RuleChanges = {
      ...template("shot {n}"),
      counter: { start: 5, step: 5, digits: 3 },
    };
    expect(renamed(["b.jpg", "a.jpg", "c.jpg"], changes)).toEqual([
      "shot 005.jpg",
      "shot 010.jpg",
      "shot 015.jpg",
    ]);
  });

  it("writes the modified date and time in the chosen form", () => {
    expect(renamed(["a.jpg"], template("{date}_{time}"))).toEqual(["2024-03-09_07-05-03.jpg"]);
    expect(
      renamed(["a.jpg"], template("{date}T{time}", { dateStyle: "YYYYMMDD", timeStyle: "HHMM" })),
    ).toEqual(["20240309T0705.jpg"]);
    expect(formatDateToken(MODIFIED, "DD-MM-YYYY")).toBe("09-03-2024");
    expect(formatDateToken(MODIFIED, "MM-DD-YYYY")).toBe("03-09-2024");
    expect(formatDateToken(MODIFIED, "YYMMDD")).toBe("240309");
    expect(formatTimeToken(MODIFIED, "HHMMSS")).toBe("070503");
    expect(formatTimeToken(MODIFIED, "HH-MM")).toBe("07-05");
  });

  it("marks items that have no modified time for a date or time token", () => {
    const selection = selectionOf([file("a.jpg", null), file("b.jpg")]);
    const dated = newNames(selection, rulesWith(template("{date} {name}")));
    expect([...dated.undated]).toEqual([0]);
    expect(dated.names).toEqual([" a.jpg", "2024-03-09 b.jpg"]);
    expect(newNames(selection, rulesWith(template("x {name}"))).undated.size).toBe(0);
  });

  it("uses the name find and replace left", () => {
    const changes = { ...find("IMG_", ""), ...template("{parent} {name}") };
    expect(renamed(["IMG_7.jpg"], changes, "trip")).toEqual(["trip 7.jpg"]);
  });

  it("leaves other braces as typed and does nothing while empty", () => {
    expect(renamed(["a.txt"], template("{name} {draft} {N}"))).toEqual(["a {draft} {N}.txt"]);
    expect(renamed(["a.txt"], template(""))).toEqual(["a.txt"]);
    expect(renamed(["a.txt"], template("{name}-x", { enabled: false }))).toEqual(["a.txt"]);
  });

  it("repeats a token as often as it is written", () => {
    expect(renamed(["a.txt"], template("{name}{name}.{ext}.{ext}"))).toEqual(["aa.txt.txt"]);
  });
});

describe("counter", () => {
  const counter = (more: Partial<RenameRules["counter"]> = {}): RuleChanges => ({
    counter: { enabled: true, ...more },
  });

  it("pads with zeros and keeps a negative sign in front", () => {
    expect(formatCounter(7, 3)).toBe("007");
    expect(formatCounter(1234, 3)).toBe("1234");
    expect(formatCounter(-2, 3)).toBe("-002");
    expect(formatCounter(0, 1)).toBe("0");
  });

  it("counts from the start in steps, up or down", () => {
    const items = ["a", "b", "c", "d", "e"].map((name) => file(name));
    const values = (start: number, step: number, digits: number) =>
      counterValues(items, { ...DEFAULT_RULES.counter, start, step, digits });
    expect(values(1, 1, 2)).toEqual(["01", "02", "03", "04", "05"]);
    expect(values(10, -3, 2)).toEqual(["10", "07", "04", "01", "-02"]);
    expect(values(-1, -1, 3)).toEqual(["-001", "-002", "-003", "-004", "-005"]);
    expect(values(0, 0, 1)).toEqual(["0", "0", "0", "0", "0"]);
    expect(values(98, 1, 2)).toEqual(["98", "99", "100", "101", "102"]);
  });

  it("falls back to sane numbers for ones that are not", () => {
    const values = counterValues([file("a"), file("b")], {
      ...DEFAULT_RULES.counter,
      start: Number.NaN,
      step: 1.9,
      digits: 400,
    });
    expect(values).toEqual(["000000000001", "000000000002"]);
  });

  it("follows the list's order, or the order of the names", () => {
    const items = ["file10.txt", "File2.txt", "file1.txt"];
    expect(renamed(items, counter({ placement: "prefix", separator: " " }))).toEqual([
      "01 file10.txt",
      "02 File2.txt",
      "03 file1.txt",
    ]);
    expect(renamed(items, counter({ placement: "prefix", separator: " ", order: "name" }))).toEqual(
      ["03 file10.txt", "02 File2.txt", "01 file1.txt"],
    );
  });

  it("goes before or after the name, never after the extension", () => {
    expect(renamed(["a.jpg", folder("b.c")], counter())).toEqual(["a_01.jpg", "b.c_02"]);
    expect(renamed(["a.jpg"], counter({ placement: "prefix", separator: "" }))).toEqual([
      "01a.jpg",
    ]);
  });

  it("leaves placing to a template when one is set", () => {
    const changes: RuleChanges = {
      ...counter({ placement: "prefix" }),
      template: { enabled: true, pattern: "{name}" },
    };
    expect(renamed(["a.jpg"], changes)).toEqual(["a.jpg"]);
  });
});

describe("letter case", () => {
  it("changes the name and not the extension", () => {
    expect(renamed(["Holiday PHOTO.JPG"], { letterCase: "lower" })).toEqual(["holiday photo.JPG"]);
    expect(renamed(["Holiday photo.jpg"], { letterCase: "upper" })).toEqual(["HOLIDAY PHOTO.jpg"]);
  });

  it("capitalizes each word for title case", () => {
    expect(applyLetterCase("the QUICK brown_fox-jumps", "title")).toBe("The Quick Brown_Fox-Jumps");
    expect(applyLetterCase("don't stop 2nd time", "title")).toBe("Don't Stop 2nd Time");
    expect(applyLetterCase("élan vital", "title")).toBe("Élan Vital");
  });

  it("capitalizes the first letter for sentence case", () => {
    expect(applyLetterCase("QUARTERLY REPORT final", "sentence")).toBe("Quarterly report final");
    expect(applyLetterCase("2024 REPORT", "sentence")).toBe("2024 Report");
    expect(applyLetterCase("", "sentence")).toBe("");
  });

  it("keeps combining marks on their letters", () => {
    const decomposed = "e\u0301cole de\u0301ja\u0300";
    expect(applyLetterCase(decomposed, "title")).toBe("E\u0301cole De\u0301ja\u0300");
    expect(applyLetterCase(decomposed, "upper")).toBe("E\u0301COLE DE\u0301JA\u0300");
    expect(applyLetterCase("STRASSE ΟΔΟΣ", "lower")).toBe("strasse οδος");
  });

  it("leaves text alone when unchanged", () => {
    expect(applyLetterCase("MiXed", "unchanged")).toBe("MiXed");
  });
});

describe("extension", () => {
  const extension = (
    change: RenameRules["extension"]["change"],
    replacement = "",
  ): RuleChanges => ({
    extension: { change, replacement },
  });
  const items = [file("Photo.JPG"), file("notes"), file(".env"), folder("set.Old")];

  it("changes its case", () => {
    expect(renamed(items, extension("lower"))).toEqual(["Photo.jpg", "notes", ".env", "set.Old"]);
    expect(renamed(["a.tar.gz"], extension("upper"))).toEqual(["a.tar.GZ"]);
  });

  it("replaces it, adding one to files that have none", () => {
    expect(renamed(items, extension("replace", "jpeg"))).toEqual([
      "Photo.jpeg",
      "notes.jpeg",
      ".env.jpeg",
      "set.Old",
    ]);
    expect(renamed(["a.txt"], extension("replace", " .md "))).toEqual(["a.md"]);
    expect(renamed(["a.txt"], extension("replace", ""))).toEqual(["a.txt"]);
  });

  it("removes it", () => {
    expect(renamed(items, extension("remove"))).toEqual(["Photo", "notes", ".env", "set.Old"]);
    expect(renamed(["a.tar.gz"], extension("remove"))).toEqual(["a.tar"]);
  });
});

describe("trim and tidy", () => {
  const tidy = (more: Partial<RenameRules["tidy"]> = {}): RuleChanges => ({
    tidy: { enabled: true, trim: false, collapseSpaces: false, replaceSpaces: false, ...more },
  });

  it("trims the ends of the name and of the extension", () => {
    expect(renamed(["  final report .pdf ", " notes "], tidy({ trim: true }))).toEqual([
      "final report.pdf",
      "notes",
    ]);
  });

  it("collapses runs of spaces, other kinds of space included", () => {
    expect(renamed(["a  b \u00a0\tc.txt"], tidy({ collapseSpaces: true }))).toEqual(["a b c.txt"]);
  });

  it("replaces each space with the chosen text, or removes it", () => {
    expect(renamed(["a b  c.txt"], tidy({ replaceSpaces: true, spaceReplacement: "_" }))).toEqual([
      "a_b__c.txt",
    ]);
    expect(renamed(["a b  c.txt"], tidy({ replaceSpaces: true, spaceReplacement: "" }))).toEqual([
      "abc.txt",
    ]);
  });

  it("trims, then collapses, then replaces", () => {
    const all = tidy({ trim: true, collapseSpaces: true, replaceSpaces: true });
    expect(renamed(["  my   summer  trip .jpg"], all)).toEqual(["my_summer_trip.jpg"]);
  });

  it("does nothing while off", () => {
    expect(renamed([" a .txt"], { tidy: { enabled: false, trim: true } })).toEqual([" a .txt"]);
  });
});

describe("the order of the rules", () => {
  it("runs find, template, case, extension, then tidying", () => {
    const changes: RuleChanges = {
      ...regex("^IMG_0*(\\d+)$", "$1"),
      template: { enabled: true, pattern: " {parent}  shot {name} of {n} " },
      counter: { start: 1, step: 1, digits: 2 },
      letterCase: "title",
      extension: { change: "lower", replacement: "" },
      tidy: { enabled: true, trim: true, collapseSpaces: true, replaceSpaces: true },
    };
    expect(renamed(["IMG_0007.JPG", "IMG_0012.JPG"], changes, "rome")).toEqual([
      "Rome_Shot_7_Of_01.jpg",
      "Rome_Shot_12_Of_02.jpg",
    ]);
  });

  it("applies the counter before letter case and tidying", () => {
    const changes: RuleChanges = {
      counter: { enabled: true, placement: "prefix", separator: " ", digits: 1 },
      letterCase: "upper",
      tidy: { enabled: true, replaceSpaces: true, spaceReplacement: "-" },
    };
    expect(renamed(["my file.txt"], changes)).toEqual(["1-MY-FILE.txt"]);
  });
});

describe("nameProblem", () => {
  const problemOf = (name: string, target = POSIX) => nameProblem(name, target)?.problem ?? null;

  it("refuses what the backend's own check refuses", () => {
    // The cases of `rejects_names_that_are_paths_on_any_platform` in src-tauri/src/local.rs.
    for (const name of [".", "..", "/", "a/", "/etc", "../escaped", "a\0b"]) {
      expect(problemOf(name), JSON.stringify(name)).toBe("invalid");
    }
    expect(problemOf("")).toBe("empty");
    for (const name of ["...", ".hidden", "ok name.txt", "naïve", "a\\b", "a:b", "what?.txt"]) {
      expect(problemOf(name), name).toBeNull();
    }
  });

  it("refuses what Windows refuses", () => {
    // The cases of `rejects_windows_drive_prefixes_and_streams` in src-tauri/src/local.rs.
    const refused = [
      "C:x.dll",
      "C:",
      "c:\\x",
      "\\\\?\\C:\\x",
      "file.txt:stream",
      "a:b",
      "..\\evil.dll",
      "a\\b",
      "\\x",
      "what?.txt",
      "a*b",
      'say "hi"',
      "a<b",
      "a|b",
      "trailing.",
      "trailing ",
      "CON",
      "nul.txt",
      "Com1.log",
      "LPT9",
    ];
    for (const name of refused) expect(problemOf(name, WINDOWS), name).toBe("invalid");
    for (const name of ["console.txt", "com10", ".git", "a b.txt", "null"]) {
      expect(problemOf(name, WINDOWS), name).toBeNull();
    }
    expect(nameProblem("nul.txt", WINDOWS)?.message).toBe("NUL is a reserved name on Windows");
    expect(nameProblem("a:b", WINDOWS)?.message).toBe("Names cannot contain :");
  });

  it("refuses control characters everywhere", () => {
    for (const name of ["a\tb", "line\nbreak", "bell\u0007", "del\u007f"]) {
      expect(problemOf(name), JSON.stringify(name)).toBe("invalid");
    }
  });

  it("counts bytes where the target does, and UTF-16 units where it does not", () => {
    expect(problemOf("a".repeat(255))).toBeNull();
    expect(problemOf("a".repeat(256))).toBe("tooLong");
    // 128 letters of two bytes each are 256 bytes but 128 characters.
    expect(problemOf("é".repeat(127))).toBeNull();
    expect(problemOf("é".repeat(128))).toBe("tooLong");
    expect(problemOf("é".repeat(255), WINDOWS)).toBeNull();
    expect(problemOf("é".repeat(256), WINDOWS)).toBe("tooLong");
    // An emoji is four bytes and two UTF-16 units.
    expect(problemOf("😀".repeat(63))).toBeNull();
    expect(problemOf("😀".repeat(64))).toBe("tooLong");
    expect(problemOf("😀".repeat(127), WINDOWS)).toBeNull();
    expect(problemOf("😀".repeat(128), WINDOWS)).toBe("tooLong");
    expect(nameProblem("é".repeat(128), POSIX)?.message).toBe(
      "The name is too long: 256 of 255 bytes",
    );
  });
});

describe("targetTraits", () => {
  const traits = (protocol: Parameters<typeof targetTraits>[0]["protocol"], folderPath = "/x") =>
    targetTraits({ pathStyle: "posix", protocol, folder: folderPath });

  it("knows Windows by its paths", () => {
    const windows = targetTraits({ pathStyle: "windows", folder: "C:\\Users" });
    expect(windows.caseInsensitive).toBe(true);
    expect(windows.windowsNames).toBe(true);
    expect(windows.lengthUnit).toBe("characters");
  });

  it("leaves case to the folder where the file system decides", () => {
    expect(targetTraits({ pathStyle: "posix", folder: "/home" }).caseInsensitive).toBeUndefined();
    for (const protocol of ["sftp", "ftp", "ftps", "ftpsImplicit", "webDav", "webDavs"] as const) {
      expect(traits(protocol).caseInsensitive, protocol).toBeUndefined();
    }
  });

  it("knows the services whose rules are fixed", () => {
    expect(traits("oneDrive").caseInsensitive).toBe(true);
    expect(traits("oneDrive").forbidden).toContain("?");
    expect(traits("box").caseInsensitive).toBe(true);
    expect(traits("box").forbidden).toBe("/\\");
    for (const protocol of ["s3", "backblazeB2", "googleDrive", "cryptomator"] as const) {
      expect(traits(protocol).caseInsensitive, protocol).toBe(false);
    }
    expect(traits("cryptomator").normalizes).toBe(true);
    expect(traits("sftp").normalizes).toBe(false);
  });

  it("leaves a name on object storage what the folder's key does not use", () => {
    expect(traits("s3", "/bucket").maxLength).toBe(1024);
    expect(traits("s3", "/bucket/photos/2024").maxLength).toBe(1024 - "photos/2024/".length);
    expect(traits("s3", "/bucket/é").maxLength).toBe(1024 - 3);
    expect(traits("backblazeB2", "/bucket/a").maxLength).toBe(250);
    expect(traits("backblazeB2", `/bucket/${"a".repeat(900)}`).maxLength).toBe(1024 - 901);
  });
});

describe("caseProbe", () => {
  it("picks a listed name whose swapped spelling is not listed", () => {
    expect(caseProbe(["2024", "Notes.txt"])).toEqual({
      listed: "Notes.txt",
      swapped: "nOTES.TXT",
    });
  });

  it("skips names that have both spellings and names without letters", () => {
    expect(caseProbe(["ab", "AB", "123", "żółć", "x"])).toEqual({ listed: "x", swapped: "X" });
    expect(caseProbe(["123", "_", "żółć"])).toBeNull();
    expect(caseProbe([])).toBeNull();
  });
});

describe("nameKey", () => {
  it("tells names apart exactly where case and form matter", () => {
    expect(nameKey("Photo.JPG", POSIX)).toBe("Photo.JPG");
    expect(nameKey("e\u0301", POSIX)).not.toBe(nameKey("\u00e9", POSIX));
  });

  it("folds case and Unicode form where the target does", () => {
    expect(nameKey("Photo.JPG", WINDOWS)).toBe(nameKey("photo.jpg", WINDOWS));
    expect(nameKey("e\u0301", VAULT)).toBe(nameKey("\u00e9", VAULT));
    expect(nameKey("A", VAULT)).not.toBe(nameKey("a", VAULT));
  });
});

describe("previewRenames", () => {
  const statuses = (preview: ReturnType<typeof previewRenames>) =>
    preview.rows.map((row) => row.problem ?? row.status);

  it("marks rows that keep their name as unchanged and counts the rest", () => {
    const preview = previewRenames(
      selectionOf(["IMG_1.jpg", "notes.txt", "IMG_2.jpg"]),
      rulesWith(find("IMG", "photo")),
      POSIX,
    );
    expect(statuses(preview)).toEqual(["ready", "unchanged", "ready"]);
    expect(preview.rows[0]).toMatchObject({ name: "IMG_1.jpg", newName: "photo_1.jpg" });
    expect(preview).toMatchObject({ ready: 2, unchanged: 1, problemCount: 0 });
    expect(problemSummary(preview)).toBeNull();
    expect(renamesOf(preview)).toEqual([
      { from: "IMG_1.jpg", to: "photo_1.jpg" },
      { from: "IMG_2.jpg", to: "photo_2.jpg" },
    ]);
  });

  it("flags two items that would end with the same name", () => {
    const preview = previewRenames(
      selectionOf(["a1.txt", "a2.txt", "b.txt"]),
      rulesWith(regex("\\d", "")),
      POSIX,
    );
    expect(statuses(preview)).toEqual(["duplicate", "duplicate", "unchanged"]);
    expect(preview.rows[0].message).toBe("a2.txt gets the same name");
    expect(preview.rows[1].message).toBe("a1.txt gets the same name");
    expect(preview.problems).toEqual({ duplicate: 2 });
    expect(problemSummary(preview)).toBe("2 items would share a name.");
    expect(renamesOf(preview)).toEqual([]);
  });

  it("flags a name a selected item keeps", () => {
    const preview = previewRenames(
      selectionOf(["draft.txt", "final.txt"]),
      rulesWith(find("draft", "final")),
      POSIX,
    );
    expect(statuses(preview)).toEqual(["duplicate", "unchanged"]);
    expect(preview.rows[0].message).toBe("final.txt keeps this name");
  });

  it("flags names that differ only in case where case does not matter", () => {
    const selection = selectionOf(["a.txt", "b.txt"]);
    const rules = rulesWith(regex("^a$", "B"));
    expect(statuses(previewRenames(selection, rules, POSIX))).toEqual(["ready", "unchanged"]);
    const preview = previewRenames(selection, rules, WINDOWS);
    expect(statuses(preview)).toEqual(["duplicate", "unchanged"]);
    expect(preview.rows[0].message).toBe("b.txt keeps a name, which differs only in case");

    const clash = previewRenames(
      selectionOf(["x-photo.txt", "y-PHOTO.txt"]),
      rulesWith(regex("^.-", "")),
      WINDOWS,
    );
    expect(statuses(clash)).toEqual(["duplicate", "duplicate"]);
    expect(clash.rows[0].message).toBe("y-PHOTO.txt gets a name, which differs only in case");
  });

  it("flags a name an item outside the selection has", () => {
    const selection = selectionOf(["draft.txt"], ["Final.txt", "other.txt"]);
    const exact = previewRenames(selection, rulesWith(find("draft", "Final")), POSIX);
    expect(statuses(exact)).toEqual(["taken"]);
    expect(exact.rows[0].message).toBe("An item that is not selected has this name");
    expect(problemSummary(exact)).toBe("1 name is already taken.");

    const lower = rulesWith(find("draft", "final", { caseSensitive: true }));
    expect(statuses(previewRenames(selection, lower, POSIX))).toEqual(["ready"]);
    const folded = previewRenames(selection, lower, WINDOWS);
    expect(statuses(folded)).toEqual(["taken"]);
    expect(folded.rows[0].message).toBe(
      "Final.txt is not selected and has a name, which differs only in case",
    );
  });

  it("lets items trade names and pass names along", () => {
    const swap = previewRenames(
      selectionOf(["2.txt", "1.txt"]),
      rulesWith({ template: { enabled: true, pattern: "{n}" }, counter: { digits: 1 } }),
      POSIX,
    );
    expect(swap.rows.map((row) => row.newName)).toEqual(["1.txt", "2.txt"]);
    expect(statuses(swap)).toEqual(["ready", "ready"]);

    const chain = previewRenames(
      selectionOf(["shot1.jpg", "shot2.jpg", "shot3.jpg"]),
      rulesWith({
        ...regex("\\d", ""),
        template: { enabled: true, pattern: "{name}{n}" },
        counter: { start: 2, digits: 1 },
      }),
      POSIX,
    );
    expect(chain.rows.map((row) => row.newName)).toEqual(["shot2.jpg", "shot3.jpg", "shot4.jpg"]);
    expect(statuses(chain)).toEqual(["ready", "ready", "ready"]);
  });

  it("lets an item change only the case of its own name anywhere", () => {
    for (const target of [POSIX, WINDOWS]) {
      const preview = previewRenames(
        selectionOf(["readme.MD"], ["other.md"]),
        rulesWith({ letterCase: "upper", extension: { change: "lower" } }),
        target,
      );
      expect(preview.rows[0]).toMatchObject({ newName: "README.md", status: "ready" });
    }
  });

  it("holds two spellings of one name to be one name in a vault", () => {
    const selection = selectionOf(["cafe1.txt", "x.txt"], ["caf\u00e9.txt"]);
    const rules = rulesWith(find("cafe1", "cafe\u0301"));
    expect(statuses(previewRenames(selection, rules, POSIX))).toEqual(["ready", "unchanged"]);
    const preview = previewRenames(selection, rules, VAULT);
    expect(statuses(preview)).toEqual(["taken", "unchanged"]);
    expect(preview.rows[0].message).toContain("differs only in how its accents are stored");
  });

  it("flags empty, invalid and overlong names before collisions", () => {
    const preview = previewRenames(
      selectionOf([file("a.txt"), file("b.txt"), file("c"), file("dd")]),
      rulesWith({
        ...regex("^(a|b)$", ""),
        extension: { change: "remove" },
        template: { enabled: false },
      }),
      POSIX,
    );
    expect(statuses(preview)).toEqual(["empty", "empty", "unchanged", "unchanged"]);
    expect(problemSummary(preview)).toBe("2 names are empty.");

    const invalid = previewRenames(
      selectionOf(["a.txt", "b.txt"]),
      rulesWith(find("a", "x/y")),
      POSIX,
    );
    expect(statuses(invalid)).toEqual(["invalid", "unchanged"]);
    expect(invalid.rows[0].message).toBe("Names cannot contain /");

    const long = previewRenames(
      selectionOf(["a.txt"]),
      rulesWith(find("a", "a".repeat(300))),
      POSIX,
    );
    expect(statuses(long)).toEqual(["tooLong"]);
    expect(problemSummary(long)).toBe("1 name is too long.");
  });

  it("flags items without a modified time when the template needs one", () => {
    const preview = previewRenames(
      selectionOf([file("a.jpg", null), file("b.jpg")]),
      rulesWith({ template: { enabled: true, pattern: "{date} {name}" } }),
      POSIX,
    );
    expect(statuses(preview)).toEqual(["undated", "ready"]);
    expect(problemSummary(preview)).toBe("1 item has no modified time.");
  });

  it("carries an invalid regular expression", () => {
    const broken = previewRenames(selectionOf(["a.txt"]), rulesWith(regex("[", "x")), POSIX);
    expect(broken.findError).toBeTruthy();
    expect(broken.ready).toBe(0);
    expect(problemSummary(broken)).toMatch(/^The regular expression is not valid: /);
  });

  it("gives each row its own problem and sums them up in one line", () => {
    const preview = previewRenames(
      selectionOf(
        ["x-same.txt", "same.txt", "x-taken.txt", "x-", "x-..", "x-fine.txt"],
        ["taken.txt"],
      ),
      rulesWith(regex("^x-", "")),
      POSIX,
    );
    expect(statuses(preview)).toEqual([
      "duplicate",
      "unchanged",
      "taken",
      "empty",
      "invalid",
      "ready",
    ]);
    expect(preview).toMatchObject({ ready: 1, unchanged: 1, problemCount: 4 });
    expect(problemSummary(preview)).toBe(
      "1 name is empty, 1 name cannot be used here, 1 item would share a name, " +
        "1 name is already taken.",
    );
    expect(renamesOf(preview)).toEqual([{ from: "x-fine.txt", to: "fine.txt" }]);
  });
});

describe("problemSummary", () => {
  it("names each kind once, in a fixed order", () => {
    const summary = problemSummary({
      rows: [],
      ready: 0,
      unchanged: 0,
      problems: { taken: 2, empty: 1, duplicate: 3, invalid: 1, tooLong: 2, undated: 4 },
      problemCount: 13,
      findError: null,
    });
    expect(summary).toBe(
      "1 name is empty, 1 name cannot be used here, 2 names are too long, " +
        "4 items have no modified time, 3 items would share a name, 2 names are already taken.",
    );
  });
});

describe("changedRange", () => {
  const highlighted = (before: string, after: string) => {
    const [start, end] = changedRange(before, after);
    return after.slice(start, end);
  };

  it("finds what lies between the shared start and end", () => {
    expect(highlighted("IMG_0001.jpg", "photo_0001.jpg")).toBe("photo");
    expect(highlighted("report.txt", "report-final.txt")).toBe("-final");
    expect(highlighted("report-final.txt", "report.txt")).toBe("");
    expect(highlighted("a.txt", "a.md")).toBe("md");
    expect(highlighted("abc", "abc")).toBe("");
    expect(highlighted("a", "aa")).toBe("a");
    expect(highlighted("", "new")).toBe("new");
    expect(highlighted("old", "")).toBe("");
  });

  it("marks a number that changed in part as a whole", () => {
    expect(highlighted("IMG_0420.JPG", "lisbon_2026-09-30.JPG")).toBe("lisbon_2026-09-30");
    expect(highlighted("a1.txt", "a12.txt")).toBe("12");
    expect(highlighted("shot 9.png", "shot 10.png")).toBe("10");
    expect(highlighted("IMG_00412.JPG", "IMG_0412.JPG")).toBe("0412");
    expect(highlighted("track1.mp3", "track1 live.mp3")).toBe(" live");
    expect(highlighted("2024.txt", "2024.txt")).toBe("");
  });

  it("never cuts through an emoji, an accent or a joined emoji", () => {
    // The two faces share their first UTF-16 unit.
    expect(highlighted("a😀.txt", "a😁.txt")).toBe("😁");
    expect(highlighted("😀b", "😁b")).toBe("😁");
    // The same base letter with another accent.
    expect(highlighted("cafe\u0301.txt", "cafe\u0300.txt")).toBe("e\u0300");
    expect(highlighted("x\u0301e\u0301", "y\u0301e\u0301")).toBe("y\u0301");
    expect(highlighted("👩\u200d💻.png", "👩\u200d🎨.png")).toBe("👩\u200d🎨");
  });
});

/** A folder that renames the way the backend does: never onto a name that is taken. */
function runPlan(names: string[], groups: RenameGroup[], target = POSIX): string[] {
  const held = new Map(names.map((name) => [nameKey(name, target), name]));
  for (const step of groups.flatMap(stepsOf)) {
    const source = nameKey(step.from, target);
    const destination = nameKey(step.to, target);
    if (held.get(source) !== step.from) throw new Error(`${step.from} is not there`);
    if (held.has(destination)) throw new Error(`${step.to} is taken`);
    held.delete(source);
    held.set(destination, step.to);
  }
  return [...held.values()].sort();
}

const temporary = (attempt: number) => `.tmp-${attempt}`;

function plan(renames: PlannedRename[], taken: string[], target = POSIX): RenameGroup[] {
  return planRenames(renames, taken, target, temporary);
}

describe("planRenames", () => {
  it("keeps independent renames in their order, one to a group", () => {
    const renames = [
      { from: "a", to: "x" },
      { from: "b", to: "y" },
    ];
    expect(plan(renames, ["a", "b", "c"])).toEqual([
      { renames: [renames[0]] },
      { renames: [renames[1]] },
    ]);
  });

  it("runs a chain from its free end", () => {
    const renames = [
      { from: "a", to: "b" },
      { from: "b", to: "c" },
      { from: "c", to: "d" },
    ];
    const groups = plan(renames, ["a", "b", "c"]);
    expect(groups.flatMap(stepsOf)).toEqual([renames[2], renames[1], renames[0]]);
    expect(groups.every((group) => group.temporary === undefined)).toBe(true);
    expect(runPlan(["a", "b", "c"], groups)).toEqual(["b", "c", "d"]);
  });

  it("runs a chain right whichever rename comes first", () => {
    const renames = [
      { from: "b", to: "c" },
      { from: "c", to: "d" },
      { from: "a", to: "b" },
    ];
    const groups = plan(renames, ["a", "b", "c"]);
    expect(groups.flatMap(stepsOf)).toEqual([renames[1], renames[0], renames[2]]);
    expect(runPlan(["a", "b", "c"], groups)).toEqual(["b", "c", "d"]);
  });

  it("opens a swap with a temporary name", () => {
    const renames = [
      { from: "a", to: "b" },
      { from: "b", to: "a" },
    ];
    const groups = plan(renames, ["a", "b"]);
    expect(groups).toEqual([{ temporary: ".tmp-1", renames }]);
    expect(stepsOf(groups[0])).toEqual([
      { from: "a", to: ".tmp-1" },
      { from: "b", to: "a" },
      { from: ".tmp-1", to: "b" },
    ]);
    expect(runPlan(["a", "b"], groups)).toEqual(["a", "b"]);
  });

  it("opens a ring of three with one temporary name", () => {
    const renames = [
      { from: "a", to: "b" },
      { from: "b", to: "c" },
      { from: "c", to: "a" },
    ];
    const groups = plan(renames, ["a", "b", "c"]);
    expect(groups).toHaveLength(1);
    expect(stepsOf(groups[0])).toEqual([
      { from: "a", to: ".tmp-1" },
      { from: "c", to: "a" },
      { from: "b", to: "c" },
      { from: ".tmp-1", to: "b" },
    ]);
    expect(runPlan(["a", "b", "c"], groups)).toEqual(["a", "b", "c"]);
  });

  it("handles rings, chains and single renames together", () => {
    const renames = [
      { from: "a", to: "b" },
      { from: "p", to: "q" },
      { from: "b", to: "a" },
      { from: "q", to: "r" },
      { from: "x", to: "y" },
      { from: "y", to: "z" },
      { from: "z", to: "x" },
      { from: "solo", to: "alone" },
    ];
    const names = ["a", "b", "p", "q", "x", "y", "z", "solo", "bystander"];
    const groups = plan(renames, names);
    expect(groups.filter((group) => group.temporary !== undefined)).toHaveLength(2);
    expect(groups.flatMap((group) => group.renames)).toHaveLength(renames.length);
    expect(runPlan(names, groups)).toEqual(
      ["b", "a", "q", "r", "y", "z", "x", "alone", "bystander"].sort(),
    );
  });

  it("goes through a temporary name to change only the case where case does not matter", () => {
    const renames = [{ from: "readme.md", to: "README.md" }];
    expect(plan(renames, ["readme.md"], POSIX)).toEqual([{ renames }]);
    const groups = plan(renames, ["readme.md"], WINDOWS);
    expect(groups).toEqual([{ temporary: ".tmp-1", renames }]);
    expect(runPlan(["readme.md"], groups, WINDOWS)).toEqual(["README.md"]);
  });

  it("follows chains and rings through names that differ in case", () => {
    const renames = [
      { from: "a.txt", to: "B.TXT" },
      { from: "b.txt", to: "C.txt" },
      { from: "one", to: "TWO" },
      { from: "two", to: "ONE" },
    ];
    const names = ["a.txt", "b.txt", "one", "two"];
    const groups = plan(renames, names, WINDOWS);
    expect(groups.flatMap(stepsOf).slice(0, 2)).toEqual([renames[1], renames[0]]);
    expect(runPlan(names, groups, WINDOWS)).toEqual(["B.TXT", "C.txt", "ONE", "TWO"]);
  });

  it("picks temporary names nothing in the folder has or will have", () => {
    const renames = [
      { from: "a", to: "b" },
      { from: "b", to: "a" },
      { from: "c", to: ".tmp-2" },
      { from: "x", to: "y" },
      { from: "y", to: "x" },
    ];
    const names = ["a", "b", "c", "x", "y", ".tmp-1", ".TMP-3"];
    const groups = plan(renames, names, WINDOWS);
    expect(groups.map((group) => group.temporary)).toEqual([".tmp-4", undefined, ".tmp-5"]);
    expect(runPlan(names, groups, WINDOWS)).toEqual(
      [".TMP-3", ".tmp-1", ".tmp-2", "a", "b", "x", "y"].sort(),
    );
  });

  it("refuses renames that share a source or a target", () => {
    expect(() =>
      plan(
        [
          { from: "a", to: "x" },
          { from: "a", to: "y" },
        ],
        ["a"],
      ),
    ).toThrow("a is renamed twice");
    expect(() =>
      plan(
        [
          { from: "a", to: "x" },
          { from: "b", to: "X" },
        ],
        ["a", "b"],
        WINDOWS,
      ),
    ).toThrow("Two items would be named X");
  });

  it("orders any shuffle of names safely", () => {
    // A small generator with a fixed seed, so a failure can be reproduced.
    let seed = 20241123;
    const random = (limit: number) => {
      seed = (seed * 1103515245 + 12345) % 2147483648;
      return seed % limit;
    };
    for (let round = 0; round < 200; round += 1) {
      const count = 2 + random(12);
      const names = Array.from({ length: count }, (_, index) => `item-${index}`);
      const pool = [...names, ...names.map((name) => `${name}-new`)];
      const targets: string[] = [];
      while (targets.length < count) {
        const [pick] = pool.splice(random(pool.length), 1);
        targets.push(pick);
      }
      const renames = names
        .map((from, index) => ({ from, to: targets[index] }))
        .filter((rename) => rename.from !== rename.to);
      const expected = names.map((name, index) => targets[index] ?? name).sort();
      expect(runPlan(names, plan(renames, names)), `round ${round}`).toEqual(expected);
    }
  });

  it("plans fifty thousand renames, as one long chain and as one long ring", () => {
    const count = 50_000;
    const name = (index: number) => `photo-${String(index).padStart(5, "0")}.jpg`;
    const names = Array.from({ length: count }, (_, index) => name(index));
    const chain = names.map((from, index) => ({ from, to: name(index + 1) }));
    const chained = plan(chain, names);
    expect(chained).toHaveLength(count);
    expect(chained[0].renames[0]).toEqual({ from: name(count - 1), to: name(count) });
    expect(runPlan(names, chained)).toHaveLength(count);

    const ring = names.map((from, index) => ({ from, to: name((index + 1) % count) }));
    const ringed = plan(ring, names);
    expect(ringed).toHaveLength(1);
    expect(stepsOf(ringed[0])).toHaveLength(count + 1);
    expect(runPlan(names, ringed)).toEqual(names);
  });
});

describe("a preview of fifty thousand items", () => {
  it("builds, validates and plans without slowing to a crawl", () => {
    const count = 50_000;
    const items = Array.from({ length: count }, (_, index) =>
      file(`IMG_${String(index).padStart(5, "0")}.JPG`),
    );
    const selection: RenameSelection = { items, parent: "camera", others: ["keep.txt"] };
    const rules = rulesWith({
      ...regex("^IMG_(\\d+)$", "$1"),
      template: { enabled: true, pattern: "{parent}-{date}-{n}" },
      counter: { digits: 5, order: "name" },
      letterCase: "title",
      extension: { change: "lower" },
      tidy: { enabled: true },
    });
    const started = performance.now();
    const preview = previewRenames(selection, rules, WINDOWS);
    const groups = planRenames(
      renamesOf(preview),
      [...items.map((item) => item.name), ...selection.others],
      WINDOWS,
      temporary,
    );
    expect(preview.ready).toBe(count);
    expect(preview.rows[41].newName).toBe("Camera-2024-03-09-00042.jpg");
    expect(groups).toHaveLength(count);
    // Generous: this takes a fraction of a second, and only a quadratic slip would pass it.
    expect(performance.now() - started).toBeLessThan(10_000);
  });
});

describe("renameOutcomes", () => {
  const asked = [
    { from: "a.txt", to: "b.txt" },
    { from: "b.txt", to: "a.txt" },
    { from: "c.txt", to: "d.txt" },
    { from: "e.txt", to: "f.txt" },
  ];

  it("says what became of each rename, in the order asked", () => {
    const outcomes = renameOutcomes(asked, {
      renamed: [{ from: "c.txt", to: "d.txt", path: "/srv/d.txt" }],
      failure: { from: "e.txt", to: "f.txt", message: "Permission denied" },
      cancelled: false,
    });
    expect(outcomes).toEqual([
      { ...asked[0], status: "left" },
      { ...asked[1], status: "left" },
      { ...asked[2], status: "renamed" },
      { ...asked[3], status: "failed", message: "Permission denied" },
    ]);
  });

  it("names the temporary name an item is left under, even when it is also the one that failed", () => {
    const outcomes = renameOutcomes(asked, {
      renamed: [{ from: "b.txt", to: "a.txt", path: "/srv/a.txt" }],
      failure: { from: "a.txt", to: "b.txt", message: "Permission denied" },
      stranded: { from: "a.txt", to: "b.txt", temporary: "poros-1.tmp", message: "It is locked" },
      cancelled: false,
    });
    expect(outcomes.map((outcome) => outcome.status)).toEqual([
      "stranded",
      "renamed",
      "left",
      "left",
    ]);
    expect(outcomes[0].message).toBe("Now named poros-1.tmp: It is locked");
  });

  it("leaves everything as it was when a run is cancelled before it starts", () => {
    const outcomes = renameOutcomes(asked, { renamed: [], cancelled: true });
    expect(outcomes.every((outcome) => outcome.status === "left")).toBe(true);
  });
});

describe("folderName", () => {
  it("is the last part of the path, and nothing for a root or a drive", () => {
    expect(folderName("/srv/photos")).toBe("photos");
    expect(folderName("/srv/photos/")).toBe("photos");
    expect(folderName("/")).toBe("");
    expect(folderName("C:\\")).toBe("");
    expect(folderName("C:\\Users\\ana")).toBe("ana");
    expect(folderName("/bucket")).toBe("bucket");
  });
});

describe("find history", () => {
  const rule = (pattern: string, replacement = "", more: Partial<FindRule> = {}): FindRule => ({
    ...DEFAULT_RULES.find,
    pattern,
    replacement,
    ...more,
  });

  it("puts the latest pattern first, once", () => {
    let history = rememberFind([], rule("a", "b"));
    history = rememberFind(history, rule("c", "d", { mode: "regex" }));
    history = rememberFind(history, rule("a", "b", { caseSensitive: true }));
    expect(history.map((entry) => entry.pattern)).toEqual(["a", "c"]);
    expect(history[0].caseSensitive).toBe(true);
    expect(history[0]).not.toHaveProperty("enabled");
  });

  it("tells a regular expression from the same text typed plainly", () => {
    const history = rememberFind(rememberFind([], rule("a.b")), rule("a.b", "", { mode: "regex" }));
    expect(history.map((entry) => entry.mode)).toEqual(["regex", "text"]);
  });

  it("keeps nothing for an empty or switched off rule, and only so many", () => {
    expect(rememberFind([], rule(""))).toEqual([]);
    expect(rememberFind([], rule("a", "b", { enabled: false }))).toEqual([]);
    let history = rememberFind([], rule("first"));
    for (let index = 0; index < FIND_HISTORY_LIMIT + 3; index += 1) {
      history = rememberFind(history, rule(`pattern ${index}`));
    }
    expect(history).toHaveLength(FIND_HISTORY_LIMIT);
    expect(history[0].pattern).toBe(`pattern ${FIND_HISTORY_LIMIT + 2}`);
    expect(history.some((entry) => entry.pattern === "first")).toBe(false);
  });

  it("drops stored entries that are not whole and fills in what is missing", () => {
    expect(sanitizeFindHistory(undefined)).toEqual([]);
    expect(sanitizeFindHistory({ pattern: "a" })).toEqual([]);
    const kept = sanitizeFindHistory([
      { pattern: "IMG_(\\d+)", replacement: "photo-$1", mode: "regex", caseSensitive: true },
      { pattern: "" },
      { pattern: 7 },
      null,
      "text",
      { pattern: "a", mode: "glob", scope: "everything", allMatches: "yes", replacement: 5 },
      { pattern: "a", replacement: "" },
      { pattern: "x".repeat(501) },
    ]);
    expect(kept).toEqual([
      {
        mode: "regex",
        pattern: "IMG_(\\d+)",
        replacement: "photo-$1",
        caseSensitive: true,
        allMatches: true,
        scope: "name",
      },
      {
        mode: "text",
        pattern: "a",
        replacement: "",
        caseSensitive: false,
        allMatches: true,
        scope: "name",
      },
    ]);
    const many = Array.from({ length: 40 }, (_, index) => ({ pattern: `p${index}` }));
    expect(sanitizeFindHistory(many)).toHaveLength(FIND_HISTORY_LIMIT);
  });

  it("labels an entry for the list of recent patterns", () => {
    expect(findHistoryLabel(rule("IMG_", "photo-"))).toBe("IMG_ to photo-");
    expect(findHistoryLabel(rule("\\s+", "", { mode: "regex" }))).toBe("\\s+ to nothing (regex)");
  });
});
