import type { TranslatedArea } from "../catalog";

export const limits = {
  // components/LimitScheduleSettings
  speedLimitSchedule: "Zeitplan für Geschwindigkeitsbegrenzungen",
  followTheSchedule: "Dem Zeitplan folgen",
  followTheScheduleHint:
    "Andere Begrenzungen oder eine Pause zu festen Zeiten der Woche, nach der Uhrzeit dieses Computers. Wo keine Regel gilt, gelten die Begrenzungen darüber.",
  whereRulesOverlapTheOne:
    "Wo sich Regeln überschneiden, gewinnt die weiter unten in der Liste. Eine Pause hält Übertragungen in dieser Richtung an, und sie laufen nach ihrem Ende dort weiter, wo sie stehen geblieben sind. Ordnersynchronisierung, rsync-Aktualisierungen und geplante Aufgaben folgen dem Zeitplan ebenfalls, ebenso poros-cli, sofern es keine eigenen Begrenzungen erhält.",
  addRule: "Regel hinzufügen",
  theScheduleIsOffThe:
    "Der Zeitplan ist aus. Die Woche zeigt, was seine Regeln täten, sobald er an ist.",
  closeTheRule: "Regel schließen",
  editTheRule: "Regel bearbeiten",
  moveUp: "Nach oben",
  moveUp2: "{label} nach oben verschieben",
  moveDownSoItWins: "Nach unten, damit sie Vorrang vor den Regeln darüber hat",
  moveDown: "{label} nach unten verschieben",
  deleteRule: "Regel löschen",
  delete: "{label} löschen",
  nameOf: "Name von {label}",
  nameSuchAsOfficeHours: "Name, etwa „Bürozeiten“",
  daysOfTheWeek: "Wochentage",
  startTime: "Beginn",
  to: "bis",
  endTime: "Ende",
  limitTo: "Begrenzen auf",
  unlimited: "Unbegrenzt",
  noDaysSoItNever: "Keine Tage, daher gilt sie nie",
  uploadLimit: "Upload-Begrenzung",
  downloadLimit: "Download-Begrenzung",

  // components/LimitScheduleStatus
  followSchedule: "Zeitplan folgen",
  followTheSpeedLimitSchedule: "Wieder dem Zeitplan für Geschwindigkeitsbegrenzungen folgen",
  ignoreSchedule: "Zeitplan ignorieren",
  ignoreTheSpeedLimitSchedule:
    "Zeitplan für Geschwindigkeitsbegrenzungen bis zu seiner nächsten Änderung ignorieren",
  clickToChangeTheSchedule: "{scheduleTooltip}\nKlicken, um den Zeitplan zu ändern.",
  scheduleIgnoredShort: "Zeitplan ignoriert",
  scheduleIgnoredShortUntil: "Zeitplan ignoriert bis {time}",
  limitsUntilTime: "{limits} bis {time}",

  // components/LimitScheduleWeek
  speedLimitsThroughTheWeek: "Geschwindigkeitsbegrenzungen über die Woche",
  eachHourUploadsAboveDownloads: "Jede Stunde: Uploads oben, Downloads unten",
  now: "Jetzt",
  limited: "Begrenzt",
  paused: "Angehalten",
  nowSpan: "Jetzt, {span}",

  // lib/limitSchedule
  ruleNumber: "Regel {number}",
  kibPerSecond: "{kib} KiB/s",
  unlimitedInASentence: "unbegrenzt",
  pausedInASentence: "angehalten",
  uploadsAndDownloads: "Uploads und Downloads {limit}",
  uploadsThenDownloads: "Uploads {upload}, Downloads {download}",
  allDay: "Ganztägig",
  twentyFourHoursFrom: "24 Stunden ab {start}",
  midnight: "Mitternacht",
  fromToTheNextDay: "{start} bis {end} am nächsten Tag",
  fromTo: "{start} bis {end}",
  noRuleTheOrdinaryLimits: "Keine Regel, die normalen Begrenzungen",
  spanDescription: "{weekday} {start} bis {end}: {source}, {limits}",
  weekdayAndTime: "{weekday} {time}",
  waitingForTheSpeedLimitSchedule: "Wartet auf den Zeitplan für Geschwindigkeitsbegrenzungen",
  waitingForTheSpeedLimitScheduleUntil:
    "Wartet bis {time} auf den Zeitplan für Geschwindigkeitsbegrenzungen",
  scheduleIgnored:
    "Zeitplan für Geschwindigkeitsbegrenzungen ignoriert: „{rule}“ ist ausgesetzt, es gelten die normalen Begrenzungen, {limits}.",
  scheduleApplies: "Zeitplan für Geschwindigkeitsbegrenzungen: „{rule}“ gilt, {limits}.",
  scheduleIgnoredUntil:
    "Zeitplan für Geschwindigkeitsbegrenzungen bis {time} ignoriert: „{rule}“ ist ausgesetzt, es gelten die normalen Begrenzungen, {limits}.",
  scheduleAppliesUntil:
    "Zeitplan für Geschwindigkeitsbegrenzungen: „{rule}“ gilt bis {time}, {limits}.",
} satisfies TranslatedArea<"limits">;
