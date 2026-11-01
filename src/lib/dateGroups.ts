// The groups Windows Explorer puts files in when it groups them by date modified: Today,
// Yesterday, Earlier this week and so on back to A long time ago. They follow the calendar, so
// a file from just before midnight is Yesterday however few hours ago that was.

import type { FileEntry } from "./types";

/** Newest first. */
export const DATE_GROUPS = [
  "future",
  "today",
  "yesterday",
  "earlierThisWeek",
  "lastWeek",
  "earlierThisMonth",
  "lastMonth",
  "earlierThisYear",
  "longAgo",
  "unknown",
] as const;
export type DateGroup = (typeof DATE_GROUPS)[number];

export const DATE_GROUP_LABELS: Record<DateGroup, string> = {
  future: "In the future",
  today: "Today",
  yesterday: "Yesterday",
  earlierThisWeek: "Earlier this week",
  lastWeek: "Last week",
  earlierThisMonth: "Earlier this month",
  lastMonth: "Last month",
  earlierThisYear: "Earlier this year",
  longAgo: "A long time ago",
  unknown: "Date unknown",
};

export interface EntryGroup {
  id: DateGroup;
  entries: FileEntry[];
}

interface WeekInfo {
  /** 1 is Monday, 7 is Sunday. */
  firstDay: number;
}

/**
 * The day weeks start on where `locale` is used, counted as `Date.getDay` counts: 0 is
 * Sunday. Monday when the engine cannot say.
 */
export function firstDayOfWeek(locale: string): number {
  try {
    const details = new Intl.Locale(locale) as Intl.Locale & {
      getWeekInfo?: () => WeekInfo;
      weekInfo?: WeekInfo;
    };
    const firstDay = details.getWeekInfo?.().firstDay ?? details.weekInfo?.firstDay;
    if (firstDay !== undefined && firstDay >= 1 && firstDay <= 7) return firstDay % 7;
  } catch {
    // An unknown locale falls back like an engine without week data.
  }
  return 1;
}

/**
 * Finds the group of a modification time in seconds, for lists shown on `today`. Each group
 * starts at a moment and the first one reached going back in time wins, so Yesterday stays
 * Yesterday when it was also last week or last month.
 */
export function dateGroupFinder(
  today: Date,
  weekStartsOn: number,
): (modified: number | null) => DateGroup {
  const year = today.getFullYear();
  const month = today.getMonth();
  const day = today.getDate();
  const startOfDay = (daysFromToday: number) =>
    new Date(year, month, day + daysFromToday).getTime();
  const daysIntoWeek = (today.getDay() - weekStartsOn + 7) % 7;
  const starts: [DateGroup, number][] = [
    ["future", startOfDay(1)],
    ["today", startOfDay(0)],
    ["yesterday", startOfDay(-1)],
    ["earlierThisWeek", startOfDay(-daysIntoWeek)],
    ["lastWeek", startOfDay(-daysIntoWeek - 7)],
    ["earlierThisMonth", new Date(year, month, 1).getTime()],
    ["lastMonth", new Date(year, month - 1, 1).getTime()],
    ["earlierThisYear", new Date(year, 0, 1).getTime()],
  ];
  return (modified) => {
    if (modified === null) return "unknown";
    const millis = modified * 1000;
    return starts.find(([, start]) => millis >= start)?.[0] ?? "longAgo";
  };
}

/**
 * Puts `entries` in date groups, keeping their order within each group. Groups with nothing in
 * them are left out, and entries with no date come last whichever way the groups run.
 */
export function groupEntriesByDate(
  entries: readonly FileEntry[],
  { today, weekStartsOn, newestFirst }: { today: Date; weekStartsOn: number; newestFirst: boolean },
): EntryGroup[] {
  const groupOf = dateGroupFinder(today, weekStartsOn);
  const buckets = new Map<DateGroup, FileEntry[]>();
  for (const entry of entries) {
    const group = groupOf(entry.modified);
    const bucket = buckets.get(group);
    if (bucket) bucket.push(entry);
    else buckets.set(group, [entry]);
  }
  const dated = DATE_GROUPS.filter((group) => group !== "unknown");
  const order: DateGroup[] = [...(newestFirst ? dated : dated.reverse()), "unknown"];
  return order.flatMap((id) => {
    const grouped = buckets.get(id);
    return grouped ? [{ id, entries: grouped }] : [];
  });
}
