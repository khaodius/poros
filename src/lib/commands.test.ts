import { describe, expect, it } from "vitest";
import {
  commandSucceeded,
  describeResult,
  expandCommand,
  needsSelection,
  shellQuote,
} from "./commands";

const target = {
  folder: "/srv/site",
  items: [
    { path: "/srv/site/index.html", name: "index.html" },
    { path: "/srv/site/it's here", name: "it's here" },
  ],
};

describe("shellQuote", () => {
  it("makes one word of anything", () => {
    expect(shellQuote("plain")).toBe("'plain'");
    expect(shellQuote("a b; rm -rf /")).toBe("'a b; rm -rf /'");
    expect(shellQuote("it's")).toBe("'it'\\''s'");
    expect(shellQuote("")).toBe("''");
  });
});

describe("expandCommand", () => {
  it("runs once per item for {path} and {name}", () => {
    expect(expandCommand("chmod 644 {path}", target)).toEqual([
      "chmod 644 '/srv/site/index.html'",
      "chmod 644 '/srv/site/it'\\''s here'",
    ]);
    expect(expandCommand("gzip -k {name}", target)).toEqual([
      "gzip -k 'index.html'",
      "gzip -k 'it'\\''s here'",
    ]);
  });

  it("runs once with every item for {paths} and {names}", () => {
    expect(expandCommand("du -sh {paths}", target)).toEqual([
      "du -sh '/srv/site/index.html' '/srv/site/it'\\''s here'",
    ]);
    expect(expandCommand("tar -czf backup.tgz {names}", target)).toEqual([
      "tar -czf backup.tgz 'index.html' 'it'\\''s here'",
    ]);
  });

  it("fills in the folder and leaves other braces to the shell", () => {
    expect(expandCommand("ls {folder} ${HOME} {other}", { ...target, items: [] })).toEqual([
      "ls '/srv/site' ${HOME} {other}",
    ]);
  });

  it("knows which commands need a selection", () => {
    expect(needsSelection("du -sh {paths}")).toBe(true);
    expect(needsSelection("rm {name}")).toBe(true);
    expect(needsSelection("git -C {folder} pull")).toBe(false);
    expect(needsSelection("uptime")).toBe(false);
  });
});

describe("describeResult", () => {
  const result = { exitStatus: 0, signal: null, stopped: false, elapsedMillis: 1500 };

  it("says how a command ended", () => {
    expect(describeResult(result)).toBe("Finished in 2s");
    expect(commandSucceeded(result)).toBe(true);
    expect(describeResult({ ...result, exitStatus: 2 })).toBe("Exited with status 2 after 2s");
    expect(describeResult({ ...result, exitStatus: null, stopped: true })).toBe("Stopped after 2s");
    expect(describeResult({ ...result, exitStatus: null, signal: "TERM" })).toBe(
      "Ended by signal TERM after 2s",
    );
    expect(commandSucceeded({ ...result, exitStatus: null })).toBe(true);
    expect(commandSucceeded({ ...result, signal: "KILL" })).toBe(false);
  });
});
