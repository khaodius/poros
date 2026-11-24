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

  // components/LimitScheduleStatus
  followSchedule: "Follow schedule",
  followTheSpeedLimitSchedule: "Follow the speed limit schedule again",
  ignoreSchedule: "Ignore schedule",
  ignoreTheSpeedLimitSchedule: "Ignore the speed limit schedule until its next change",
  clickToChangeTheSchedule: "{scheduleTooltip}\nClick to change the schedule.",

  // components/LimitScheduleWeek
  speedLimitsThroughTheWeek: "Speed limits through the week",
  eachHourUploadsAboveDownloads: "Each hour: uploads above, downloads below",
  now: "Now",
  // components/LimitScheduleSettings
  limitTo: "Limit to",
  unlimited: "Unlimited",

  // components/LimitScheduleWeek
  limited: "Limited",
  paused: "Paused",
  // components/LimitScheduleSettings
  noDaysSoItNever: "No days, so it never applies",

  // components/LimitScheduleStatus
  scheduleIgnored: "Schedule ignored{until}",
} as const satisfies MessageArea;
