import { describe, expect, it } from "vitest";
import casesText from "../../src-tauri/tests/fixtures/limit-schedule-cases.json?raw";
import {
  MAX_LIMIT_KIB,
  MAX_RULES,
  PAUSED,
  UNLIMITED,
  daySpans,
  describeHours,
  describeSpan,
  hourSpans,
  clockTime,
  limited,
  limitsAt,
  moveRule,
  ordinaryLimits,
  sanitizeLimitSchedule,
  scheduleSetsLimits,
  scheduleTooltip,
  scheduleWait,
  shownLimits,
  type InForce,
} from "./limitSchedule";
import { sanitizeSettings } from "./settings";
import type { LimitMode, LimitRule, LimitSchedule, LimitStatus, RateLimit } from "./types";

interface Case {
  why: string;
  schedule: string;
  ordinary: [number, number];
  at: string;
  rule: number | null;
  upload: number | LimitMode;
  download: number | LimitMode;
}

const table = JSON.parse(casesText) as {
  schedules: Record<string, LimitSchedule>;
  cases: Case[];
};

function expected(value: number | LimitMode): RateLimit {
  if (typeof value === "number") return limited(value);
  return value === "paused" ? PAUSED : UNLIMITED;
}

/** The weekday (Monday is 0) and minute of a local `YYYY-MM-DDTHH:MM:SS`, as written. */
function weekdayAndMinute(at: string): [number, number] {
  const [date, time] = at.split("T");
  const [year, month, day] = date.split("-").map(Number);
  const [hours, minutes] = time.split(":").map(Number);
  const weekday = (new Date(Date.UTC(year, month - 1, day)).getUTCDay() + 6) % 7;
  return [weekday, hours * 60 + minutes];
}

const MONDAY = 0;
const TUESDAY = 1;
const office = table.schedules.office;

describe("the table of cases shared with the backend", () => {
  it("has the cases the backend reads", () => {
    expect(table.cases.length).toBeGreaterThan(40);
  });

  it.each(Object.entries(table.schedules))("stores %s as it is used", (_name, schedule) => {
    expect(sanitizeLimitSchedule(schedule)).toEqual(schedule);
  });

  it.each(table.cases)("$schedule at $at: $why", (testCase) => {
    const schedule = table.schedules[testCase.schedule];
    const [weekday, minute] = weekdayAndMinute(testCase.at);
    const ordinary = ordinaryLimits(testCase.ordinary[0], testCase.ordinary[1]);
    const inForce: InForce = {
      rule: testCase.rule,
      upload: expected(testCase.upload),
      download: expected(testCase.download),
    };
    expect(limitsAt(schedule, ordinary, weekday, minute)).toEqual(inForce);
  });
});

describe("sanitizeLimitSchedule", () => {
  it("makes stored rules safe to use, as the backend does", () => {
    const stored = sanitizeLimitSchedule({
      enabled: true,
      rules: [
        {
          id: " work ",
          name: "  Work hours \nrm -rf",
          days: [4, 0, 9, 4, 255, -1, 1.5, "2"],
          startMinute: 540,
          endMinute: 5000,
          upload: { mode: "limited", kib: 0 },
          download: { mode: "limited", kib: 4_000_000_000 },
        },
        {
          id: "work",
          name: "\u001b[2JBackup\t",
          upload: { mode: "stopped", kib: 7 },
          download: { mode: "paused" },
        },
        { name: "x".repeat(200), days: [6] },
        {},
      ],
    });
    expect(stored.enabled).toBe(true);
    const [first, second, third, fourth] = stored.rules;
    expect(first).toEqual({
      id: "work",
      name: "Work hours",
      days: [0, 4],
      startMinute: 540,
      endMinute: 1439,
      upload: UNLIMITED,
      download: limited(MAX_LIMIT_KIB),
    });
    expect(second.id).toBe("work-2");
    expect(second.name).toBe("[2JBackup");
    expect(second.upload).toEqual({ mode: "unlimited", kib: 7 });
    expect(second.download).toEqual(PAUSED);
    expect(third.id).toBe("rule-3");
    expect([...third.name]).toHaveLength(60);
    expect(fourth).toEqual({
      id: "rule-4",
      name: "",
      days: [],
      startMinute: 0,
      endMinute: 0,
      upload: UNLIMITED,
      download: UNLIMITED,
    });
    expect(sanitizeLimitSchedule(stored)).toEqual(stored);

    const many = sanitizeLimitSchedule({ enabled: true, rules: Array(MAX_RULES + 5).fill({}) });
    expect(many.rules).toHaveLength(MAX_RULES);
  });

  it("reads anything else as a schedule switched off with no rules", () => {
    for (const stored of [undefined, null, "on", [], { enabled: "yes", rules: {} }]) {
      expect(sanitizeLimitSchedule(stored)).toEqual({ enabled: false, rules: [] });
    }
  });

  it("leaves settings saved by 0.7.0 as they were, with the schedule off", () => {
    const settings = sanitizeSettings({ transfers: { uploadLimitKib: 250, workers: 4 } });
    expect(settings.transfers.uploadLimitKib).toBe(250);
    expect(settings.transfers.limitSchedule).toEqual({ enabled: false, rules: [] });
    const kept = sanitizeSettings({
      transfers: { limitSchedule: { enabled: true, rules: [{ id: "a", days: [9, 2] }] } },
    });
    expect(kept.transfers.limitSchedule.enabled).toBe(true);
    expect(kept.transfers.limitSchedule.rules[0].days).toEqual([2]);
  });
});

describe("the week view", () => {
  const ordinary = ordinaryLimits(300, 0);
  const none = (start: number, end: number) => ({
    start,
    end,
    inForce: { rule: null, ...ordinary },
  });

  it("splits a day where rules start and end, with the night of the day before", () => {
    const night = { rule: 0, upload: UNLIMITED, download: UNLIMITED };
    expect(daySpans(office, ordinary, TUESDAY)).toEqual([
      { start: 0, end: 120, inForce: night },
      { start: 120, end: 150, inForce: { rule: 2, upload: PAUSED, download: PAUSED } },
      { start: 150, end: 360, inForce: night },
      none(360, 540),
      { start: 540, end: 1080, inForce: { rule: 1, upload: limited(200), download: limited(500) } },
      none(1080, 1320),
      { start: 1320, end: 1440, inForce: night },
    ]);
  });

  it("joins stretches that come to the same", () => {
    const off: LimitSchedule = { ...office, enabled: false };
    expect(daySpans(off, ordinary, MONDAY)).toEqual([none(0, 1440)]);
    expect(daySpans({ enabled: true, rules: [] }, ordinary, MONDAY)).toEqual([none(0, 1440)]);
  });

  it("cuts the stretches to each hour", () => {
    const spans = daySpans(office, ordinary, TUESDAY);
    expect(
      hourSpans(spans, 2).map(({ start, end, inForce }) => [start, end, inForce.rule]),
    ).toEqual([
      [120, 150, 2],
      [150, 180, 0],
    ]);
    expect(hourSpans(spans, 23).map(({ start, end }) => [start, end])).toEqual([[1380, 1440]]);
  });

  it("names the rule and limits of a stretch", () => {
    const [backup] = hourSpans(daySpans(office, ordinary, TUESDAY), 2);
    expect(describeSpan(office, TUESDAY, backup)).toBe(
      "Tue 02:00 to 02:30: Backup window, uploads and downloads paused",
    );
    const [evening] = hourSpans(daySpans(office, ordinary, MONDAY), 19);
    expect(describeSpan(office, MONDAY, evening)).toBe(
      "Mon 19:00 to 20:00: No rule, the ordinary limits, uploads 300 KiB/s, downloads unlimited",
    );
    const [end] = hourSpans(daySpans(office, ordinary, MONDAY), 23);
    expect(describeSpan(office, MONDAY, end)).toBe(
      "Mon 23:00 to 24:00: Night, uploads and downloads unlimited",
    );
  });
});

describe("rule editing", () => {
  it("says when a rule runs", () => {
    expect(describeHours({ startMinute: 540, endMinute: 1080 })).toBe("09:00 to 18:00");
    expect(describeHours({ startMinute: 1320, endMinute: 360 })).toBe(
      "22:00 to 06:00 the next day",
    );
    expect(describeHours({ startMinute: 1200, endMinute: 0 })).toBe("20:00 to midnight");
    expect(describeHours({ startMinute: 720, endMinute: 720 })).toBe("24 hours from 12:00");
    expect(describeHours({ startMinute: 0, endMinute: 0 })).toBe("All day");
  });

  it("moves a rule within the list", () => {
    const rules = office.rules;
    const ids = (moved: LimitRule[]) => moved.map((rule) => rule.id);
    expect(ids(moveRule(rules, 2, -1))).toEqual(["night", "backup", "work"]);
    expect(ids(moveRule(rules, 0, 1))).toEqual(["work", "night", "backup"]);
    expect(moveRule(rules, 0, -1)).toBe(rules);
    expect(moveRule(rules, 2, 1)).toBe(rules);
  });
});

function status(change: Partial<LimitStatus>): LimitStatus {
  return {
    upload: UNLIMITED,
    download: UNLIMITED,
    rule: null,
    until: null,
    ignored: false,
    pauseEnds: { upload: null, download: null, relay: null },
    ...change,
  };
}

describe("what the windows show", () => {
  const now = new Date(2026, 10, 23, 10, 0);
  const at = (day: number, hours: number, minutes = 0) =>
    new Date(2026, 10, day, hours, minutes).getTime() / 1000;

  it("shows times as the clock does, with the weekday when not today", () => {
    expect(clockTime(at(23, 18), now)).toBe("18:00");
    expect(clockTime(at(24, 6, 5), now)).toBe("Tue 06:05");
    expect(clockTime(at(29, 0), now)).toBe("Sun 00:00");
  });

  it("says a queued transfer waits for the schedule, and until when", () => {
    const quietHour = status({
      upload: PAUSED,
      download: limited(500),
      rule: { index: 0, name: "Quiet hour" },
      until: at(23, 12),
      pauseEnds: { upload: at(23, 13), download: null, relay: at(23, 13) },
    });
    const job = { state: "queued", direction: "upload" } as const;
    expect(scheduleWait(job, quietHour, now)).toBe(
      "Waiting for the speed limit schedule until 13:00",
    );
    expect(scheduleWait({ ...job, direction: "relay" }, quietHour, now)).toBe(
      "Waiting for the speed limit schedule until 13:00",
    );
    expect(scheduleWait({ ...job, direction: "download" }, quietHour, now)).toBeNull();
    expect(scheduleWait({ ...job, state: "paused" }, quietHour, now)).toBeNull();
    const endless = { ...quietHour, pauseEnds: { upload: null, download: null, relay: null } };
    expect(scheduleWait(job, endless, now)).toBe("Waiting for the speed limit schedule");
  });

  it("names the limits a rule sets in the status bar", () => {
    const work = status({
      upload: limited(200),
      download: limited(500),
      rule: { index: 1, name: "Work hours" },
      until: at(23, 18),
    });
    expect(scheduleSetsLimits(work)).toBe(true);
    expect(shownLimits(work)).toEqual([
      { direction: "upload", mode: "limited", text: "200 KiB/s" },
      { direction: "download", mode: "limited", text: "500 KiB/s" },
    ]);
    expect(shownLimits({ ...work, upload: UNLIMITED })).toEqual([
      { direction: "download", mode: "limited", text: "500 KiB/s" },
    ]);
    expect(shownLimits({ ...work, upload: UNLIMITED, download: UNLIMITED })).toHaveLength(2);
    expect(scheduleTooltip(work, now)).toBe(
      'Speed limit schedule: "Work hours" applies until 18:00, uploads 200 KiB/s, downloads 500 KiB/s.',
    );

    const ignored = { ...work, ignored: true, upload: UNLIMITED, download: UNLIMITED };
    expect(scheduleSetsLimits(ignored)).toBe(false);
    expect(scheduleTooltip(ignored, now)).toBe(
      'Speed limit schedule ignored until 18:00: "Work hours" is set aside and the ordinary limits apply, uploads and downloads unlimited.',
    );
    expect(scheduleSetsLimits(status({}))).toBe(false);
    const unnamed = { ...work, rule: { index: 2, name: "" }, until: null };
    expect(scheduleTooltip(unnamed, now)).toBe(
      'Speed limit schedule: "Rule 3" applies, uploads 200 KiB/s, downloads 500 KiB/s.',
    );
  });
});
