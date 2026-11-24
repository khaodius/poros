import type { MessageArea } from "../catalog";

export const limits = {
  // components/LimitScheduleSettings
  speedLimitSchedule: "Speed limit schedule",
  followTheSchedule: "Follow the schedule",
  followTheScheduleHint:
    "Other limits, or a pause, at set times of the week in this computer's time. Where no rule applies, the limits above do.",
  whereRulesOverlapTheOne:
    "Where rules overlap, the one lower in the list wins. A pause holds transfers in that direction, and they continue where they stopped when it ends. Folder sync, rsync updates and scheduled tasks follow the schedule too, as does poros-cli unless it is given limits of its own.",
  addRule: "Add rule",
  theScheduleIsOffThe: "The schedule is off: the week shows what its rules would do once it is on.",
  closeTheRule: "Close the rule",
  editTheRule: "Edit the rule",
  moveUp: "Move up",
  moveUp2: "Move {label} up",
  moveDownSoItWins: "Move down, so it wins over the rules above",
  moveDown: "Move {label} down",
  deleteRule: "Delete rule",
  delete: "Delete {label}",
  nameOf: "Name of {label}",
  nameSuchAsOfficeHours: 'Name, such as "Office hours"',
  daysOfTheWeek: "Days of the week",
  startTime: "Start time",
  to: "to",
  endTime: "End time",
  limitTo: "Limit to",
  unlimited: "Unlimited",
  noDaysSoItNever: "No days, so it never applies",
  uploadLimit: "Upload limit",
  downloadLimit: "Download limit",

  // components/LimitScheduleStatus
  followSchedule: "Follow schedule",
  followTheSpeedLimitSchedule: "Follow the speed limit schedule again",
  ignoreSchedule: "Ignore schedule",
  ignoreTheSpeedLimitSchedule: "Ignore the speed limit schedule until its next change",
  clickToChangeTheSchedule: "{scheduleTooltip}\nClick to change the schedule.",
  scheduleIgnoredShort: "Schedule ignored",
  scheduleIgnoredShortUntil: "Schedule ignored until {time}",
  untilTime: "until {time}",

  // components/LimitScheduleWeek
  speedLimitsThroughTheWeek: "Speed limits through the week",
  eachHourUploadsAboveDownloads: "Each hour: uploads above, downloads below",
  now: "Now",
  limited: "Limited",
  paused: "Paused",
  nowSpan: "Now, {span}",

  // lib/limitSchedule
  ruleNumber: "Rule {number}",
  kibPerSecond: "{kib} KiB/s",
  unlimitedInASentence: "unlimited",
  pausedInASentence: "paused",
  uploadsAndDownloads: "uploads and downloads {limit}",
  uploadsThenDownloads: "uploads {upload}, downloads {download}",
  allDay: "All day",
  twentyFourHoursFrom: "24 hours from {start}",
  midnight: "midnight",
  fromToTheNextDay: "{start} to {end} the next day",
  fromTo: "{start} to {end}",
  noRuleTheOrdinaryLimits: "No rule, the ordinary limits",
  spanDescription: "{weekday} {start} to {end}: {source}, {limits}",
  weekdayAndTime: "{weekday} {time}",
  waitingForTheSpeedLimitSchedule: "Waiting for the speed limit schedule",
  waitingForTheSpeedLimitScheduleUntil: "Waiting for the speed limit schedule until {time}",
  scheduleIgnored:
    'Speed limit schedule ignored: "{rule}" is set aside and the ordinary limits apply, {limits}.',
  scheduleApplies: 'Speed limit schedule: "{rule}" applies, {limits}.',
  scheduleIgnoredUntil:
    'Speed limit schedule ignored until {time}: "{rule}" is set aside and the ordinary limits apply, {limits}.',
  scheduleAppliesUntil: 'Speed limit schedule: "{rule}" applies until {time}, {limits}.',
} as const satisfies MessageArea;
