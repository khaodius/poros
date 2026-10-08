import { describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS } from "./settings";
import {
  convertTrigger,
  dateTimeInputValue,
  describeTrigger,
  epochFromDateTimeInput,
  intervalMinutes,
  minuteOfDayFrom,
  newTask,
  sameTask,
  splitInterval,
  suggestedName,
  syncAction,
  timeInputValue,
  weekdayOf,
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
  it("builds a sync action from the dialog's options", () => {
    const action = syncAction({
      connectionId: "web",
      localPath: "/home/me/site",
      remotePath: "/var/www",
      options: {
        ...DEFAULT_SETTINGS.sync,
        direction: "both",
        deleteExtraneous: true,
        excludes: "*.tmp\n\n.git/",
      },
    });
    expect(action).toMatchObject({
      type: "sync",
      direction: "both",
      deleteExtraneous: false,
      excludes: ["*.tmp", ".git/"],
    });
  });

  it("suggests a name from what the task does", () => {
    const upload = syncAction({
      connectionId: "web",
      localPath: "C:\\Users\\me\\site\\",
      remotePath: "/var/www",
      options: DEFAULT_SETTINGS.sync,
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
