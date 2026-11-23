import { describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS } from "./settings";
import {
  convertTrigger,
  dateTimeInputValue,
  describeTrigger,
  epochFromDateTimeInput,
  expandFolderPattern,
  finishedRuns,
  intervalMinutes,
  isoWeek,
  minuteOfDayFrom,
  newTask,
  sameTask,
  serverChanges,
  splitInterval,
  suggestedName,
  syncAction,
  taskOptions,
  timeInputValue,
  transferAction,
  weekdayOf,
  withDirection,
  withTaskItems,
} from "./schedule";

describe("intervals", () => {
  it("shows an interval in the largest unit that fits evenly", () => {
    expect(splitInterval(90)).toEqual({ amount: 90, unit: "minutes" });
    expect(splitInterval(120)).toEqual({ amount: 2, unit: "hours" });
    expect(splitInterval(2 * 24 * 60)).toEqual({ amount: 2, unit: "days" });
    expect(intervalMinutes(3, "hours")).toBe(180);
  });
});

describe("times", () => {
  it("converts times of day", () => {
    expect(timeInputValue(2 * 60 + 5)).toBe("02:05");
    expect(minuteOfDayFrom("23:59")).toBe(23 * 60 + 59);
    expect(minuteOfDayFrom("24:00")).toBeNull();
    expect(minuteOfDayFrom("")).toBeNull();
  });

  it("reads and writes local dates and times", () => {
    const at = new Date(2026, 9, 8, 14, 30).getTime() / 1000;
    expect(dateTimeInputValue(at)).toBe("2026-10-08T14:30");
    expect(epochFromDateTimeInput("2026-10-08T14:30")).toBe(at);
    expect(epochFromDateTimeInput("tomorrow")).toBeNull();
  });

  it("counts weekdays from Monday", () => {
    expect(weekdayOf(new Date(2026, 9, 12))).toBe(0);
    expect(weekdayOf(new Date(2026, 9, 11))).toBe(6);
  });
});

describe("describeTrigger", () => {
  it("says when a task runs", () => {
    expect(describeTrigger({ type: "every", minutes: 60, start: 0 })).toBe("Every hour");
    expect(describeTrigger({ type: "every", minutes: 30, start: 0 })).toBe("Every 30 minutes");
    expect(describeTrigger({ type: "every", minutes: 2880, start: 0 })).toBe("Every 2 days");
    expect(
      describeTrigger({ type: "daily", minuteOfDay: 150, weekdays: [0, 1, 2, 3, 4, 5, 6] }),
    ).toBe("Every day at 02:30");
    expect(describeTrigger({ type: "daily", minuteOfDay: 0, weekdays: [4, 0, 2, 1, 3] })).toBe(
      "Weekdays at 00:00",
    );
    expect(describeTrigger({ type: "daily", minuteOfDay: 600, weekdays: [6, 2] })).toBe(
      "Wed, Sun at 10:00",
    );
    const at = new Date(2026, 9, 8, 14, 30).getTime() / 1000;
    expect(describeTrigger({ type: "once", at })).toBe("Once, 2026-10-08 14:30");
  });
});

describe("convertTrigger", () => {
  const now = new Date(2026, 9, 8, 14, 20);
  const seconds = (date: Date) => date.getTime() / 1000;

  it("keeps the time when switching kinds", () => {
    const daily = { type: "daily" as const, minuteOfDay: 9 * 60, weekdays: [0] };
    expect(convertTrigger(daily, "once", now)).toEqual({
      type: "once",
      at: seconds(new Date(2026, 9, 9, 9, 0)),
    });
    const once = { type: "once" as const, at: seconds(new Date(2026, 9, 8, 18, 45)) };
    expect(convertTrigger(once, "daily", now)).toEqual({
      type: "daily",
      minuteOfDay: 18 * 60 + 45,
      weekdays: [0, 1, 2, 3, 4, 5, 6],
    });
    expect(convertTrigger(once, "every", now)).toEqual({
      type: "every",
      minutes: 60,
      start: once.at,
    });
  });

  it("starts at the next full hour instead of in the past", () => {
    const every = { type: "every" as const, minutes: 30, start: seconds(new Date(2026, 0, 1)) };
    expect(convertTrigger(every, "once", now)).toEqual({
      type: "once",
      at: seconds(new Date(2026, 9, 8, 15, 0)),
    });
  });
});

describe("tasks", () => {
  it("builds a sync action from the dialog's options and reads them back", () => {
    const options = {
      ...DEFAULT_SETTINGS.sync.options,
      deleteExtraneous: true,
      excludes: [".git/"],
    };
    const action = syncAction({
      connectionId: "web",
      localPath: "/home/me/site",
      remotePath: "/var/www",
      direction: "upload",
      options,
      arguments: "--delete-after",
    });
    expect(action).toMatchObject({
      type: "sync",
      direction: "upload",
      deleteExtraneous: true,
      excludes: [".git/"],
      arguments: "--delete-after",
    });
    if (action.type !== "sync") throw new Error("not a sync action");
    expect(taskOptions(action)).toEqual(options);
  });

  it("suggests a name from what the task does", () => {
    const upload = syncAction({
      connectionId: "web",
      localPath: "C:\\Users\\me\\site\\",
      remotePath: "/var/www",
      ...DEFAULT_SETTINGS.sync,
    });
    expect(suggestedName(upload)).toBe("Upload site");
    expect(suggestedName({ ...upload, direction: "download" } as typeof upload)).toBe(
      "Download www",
    );
    expect(suggestedName({ type: "command", connectionId: "web", command: " df -h" })).toBe(
      "Run df",
    );
  });
});

describe("sameTask", () => {
  it("ignores key order and what the backend works out", () => {
    const task = newTask({ type: "command", connectionId: "web", command: "df", directory: null });
    const reordered = {
      lastRun: null,
      runMissed: task.runMissed,
      action: { directory: null, command: "df", connectionId: "web", type: "command" as const },
      trigger: task.trigger,
      enabled: true,
      name: "",
      id: "",
      nextRun: 5,
    };
    expect(sameTask(task, reordered)).toBe(true);
    expect(sameTask(task, { ...reordered, enabled: false })).toBe(false);
  });
});

describe("folder patterns", () => {
  // Mirrors the tests of `expand_folder_pattern` in automation/schedule.rs.
  const expand = (pattern: string, now = new Date(2026, 10, 23, 2, 5)) => {
    const result = expandFolderPattern(pattern, now);
    if ("error" in result) throw new Error(result.error);
    return result.folder;
  };
  const refused = (pattern: string) => {
    const result = expandFolderPattern(pattern, new Date(2026, 10, 23, 2, 5));
    return "error" in result ? result.error : "";
  };

  it("takes the local date and time of the run", () => {
    // 2026-11-23 is a Monday in ISO week 48.
    expect(expand("backup-{date}")).toBe("backup-2026-11-23");
    expect(expand(" /{year}/{month}/site-{day}/ ")).toBe("2026/11/site-23");
    expect(expand("{date}_{time}")).toBe("2026-11-23_0205");
    expect(expand("{hour}h{minute}")).toBe("02h05");
    expect(expand("weekly/{weekday}")).toBe("weekly/Mon");
    expect(expand("week-{week}")).toBe("week-48");
    expect(expand("plain folder")).toBe("plain folder");
    expect(expand("")).toBe("");
    expect(expand(" / ")).toBe("");
    expect(expand("{date}-{weekday}", new Date(2026, 11, 31, 23, 30))).toBe("2026-12-31-Thu");
  });

  it("refuses what cannot name a folder", () => {
    expect(refused("backup-{dat}")).toContain("{dat}");
    expect(refused("backup-{dat}")).toContain("{date}");
    expect(refused("backup-{date")).toContain("brace");
    expect(refused("backup-date}")).toContain("brace");
    expect(refused("{{date}}")).toContain("brace");
    expect(refused("../outside")).toContain('".."');
    expect(refused("a/../../b")).toContain("cannot be a folder name");
    expect(refused("a//b")).toContain("cannot be a folder name");
    expect(refused("C:\\backups")).toContain("cannot be a folder name");
    expect(refused("what?")).toContain("cannot be a folder name");
    expect(refused("line\nbreak")).toContain("cannot be a folder name");
    expect(refused("trailing./x")).toContain("cannot be a folder name");
    expect(refused("x".repeat(201))).toContain("longer");
  });

  it("counts ISO weeks from the week with the year's first Thursday", () => {
    expect(isoWeek(new Date(2026, 0, 1))).toBe(1);
    expect(isoWeek(new Date(2026, 11, 31))).toBe(53);
    expect(isoWeek(new Date(2027, 0, 3))).toBe(53);
    expect(isoWeek(new Date(2027, 0, 4))).toBe(1);
    expect(isoWeek(new Date(2024, 11, 30))).toBe(1);
  });
});

describe("transfer tasks", () => {
  const upload = transferAction("web", "upload", {
    items: [
      { path: "/home/me/site", isDir: true },
      { path: "/home/me/notes.txt", isDir: false },
    ],
    destination: "/var/www",
  });

  it("suggests a name from the items", () => {
    expect(suggestedName(transferAction("web"))).toBe("");
    expect(suggestedName({ ...upload, items: upload.items.slice(0, 1) })).toBe("Upload site");
    expect(suggestedName(withDirection(upload, "relay"))).toBe("");
    expect(suggestedName({ ...upload, direction: "relay" })).toBe("Copy 2 items");
  });

  it("keeps what still means the same when the direction changes", () => {
    const relay = withDirection(upload, "relay");
    // The items were on this computer; the destination is on a server either way.
    expect(relay).toMatchObject({ connectionId: "web", items: [], destination: "/var/www" });
    const download = withDirection(
      { ...relay, sourceConnectionId: "old", items: [{ path: "/srv/a", isDir: true }] },
      "download",
    );
    // The server copied from is the one downloaded from, with the same items.
    expect(download).toMatchObject({
      connectionId: "old",
      sourceConnectionId: null,
      items: [{ path: "/srv/a", isDir: true }],
      destination: "",
    });
    expect(withDirection(download, "relay")).toMatchObject({
      connectionId: "",
      sourceConnectionId: "old",
      items: [{ path: "/srv/a", isDir: true }],
    });
    expect(withDirection(download, "download")).toBe(download);
  });

  it("adds each path once", () => {
    const items = withTaskItems(upload.items, [
      { path: " /home/me/site ", isDir: true },
      { path: "/home/me/other.txt" },
      { path: "  " },
    ]);
    expect(items.map((item) => item.path)).toEqual([
      "/home/me/site",
      "/home/me/notes.txt",
      "/home/me/other.txt",
    ]);
  });
});

describe("what runs change", () => {
  it("names the folders a task writes into on its servers", () => {
    const upload = transferAction("web", "upload", { destination: "/var/www" });
    expect(serverChanges(upload)).toEqual([{ connectionId: "web", folder: "/var/www" }]);
    expect(serverChanges({ ...upload, direction: "relay", sourceConnectionId: "nas" })).toEqual([
      { connectionId: "web", folder: "/var/www" },
    ]);
    // A download changes this computer only.
    expect(serverChanges({ ...upload, direction: "download" })).toEqual([]);
    const sync = syncAction({
      connectionId: "web",
      localPath: "/home/me/site",
      remotePath: "/var/www/site",
      ...DEFAULT_SETTINGS.sync,
      direction: "both",
    });
    expect(serverChanges(sync)).toEqual([{ connectionId: "web", folder: "/var/www/site" }]);
    expect(serverChanges({ type: "command", connectionId: "web", command: "make" })).toEqual([
      { connectionId: "web", folder: null },
    ]);
  });

  it("finds the runs that finished since the last look", () => {
    const task = (id: string, finished: number | null) => ({
      ...newTask(transferAction("web")),
      id,
      running: false,
      lastRun:
        finished === null ? null : { started: 100, finished, succeeded: true, message: "Done" },
    });
    const before = [task("a", 200), task("b", null), task("c", 300)];
    const after = [task("a", 200), task("b", 400), task("c", 500), task("d", null)];
    expect(finishedRuns(before, after).map((found) => found.id)).toEqual(["b", "c"]);
    expect(finishedRuns(after, after)).toEqual([]);
  });

  it("does not take what a run writes as it begins for its result", () => {
    const run = (finished: number, underWay?: boolean) => ({
      started: 100,
      finished,
      succeeded: !underWay,
      message: underWay ? "Stopped before it finished" : "Done",
      ...(underWay ? { underWay } : {}),
    });
    const task = (lastRun: ReturnType<typeof run>) => ({
      ...newTask(transferAction("web")),
      id: "a",
      running: true,
      lastRun,
    });
    const before = [task({ ...run(50), started: 40 })];
    const begun = [task(run(100, true))];
    expect(finishedRuns(before, begun)).toEqual([]);
    // Its result replaces it, even in the second it began.
    expect(finishedRuns(begun, [task(run(100))]).map((found) => found.id)).toEqual(["a"]);
    expect(finishedRuns(begun, [task(run(160))]).map((found) => found.id)).toEqual(["a"]);
  });
});
