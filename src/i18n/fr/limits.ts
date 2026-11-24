import type { TranslatedArea } from "../catalog";

export const limits = {
  // components/LimitScheduleSettings
  speedLimitSchedule: "Planification des limites de vitesse",
  followTheSchedule: "Suivre la planification",
  followTheScheduleHint:
    "D’autres limites, ou une pause, à des moments fixes de la semaine, à l’heure de cet ordinateur. Là où aucune règle ne s’applique, les limites ci-dessus s’appliquent.",
  whereRulesOverlapTheOne:
    "Quand des règles se chevauchent, celle placée le plus bas dans la liste l’emporte. Une pause retient les transferts dans ce sens, et ils reprennent là où ils s’étaient arrêtés quand elle se termine. La synchronisation de dossiers, les mises à jour rsync et les tâches planifiées suivent aussi la planification, de même que poros-cli s’il n’a pas reçu ses propres limites.",
  addRule: "Ajouter une règle",
  theScheduleIsOffThe:
    "La planification est désactivée\u00a0: la semaine montre ce que ses règles feraient une fois activée.",
  closeTheRule: "Fermer la règle",
  editTheRule: "Modifier la règle",
  moveUp: "Déplacer vers le haut",
  moveUp2: "Déplacer «\u00a0{label}\u00a0» vers le haut",
  moveDownSoItWins: "Déplacer vers le bas, pour qu’elle l’emporte sur les règles du dessus",
  moveDown: "Déplacer «\u00a0{label}\u00a0» vers le bas",
  deleteRule: "Supprimer la règle",
  delete: "Supprimer «\u00a0{label}\u00a0»",
  nameOf: "Nom de «\u00a0{label}\u00a0»",
  nameSuchAsOfficeHours: "Nom, par exemple «\u00a0Heures de bureau\u00a0»",
  daysOfTheWeek: "Jours de la semaine",
  startTime: "Heure de début",
  to: "à",
  endTime: "Heure de fin",
  limitTo: "Limiter à",
  unlimited: "Illimité",
  noDaysSoItNever: "Aucun jour, elle ne s’applique donc jamais",
  uploadLimit: "Limite d’envoi",
  downloadLimit: "Limite de téléchargement",

  // components/LimitScheduleStatus
  followSchedule: "Suivre la planification",
  followTheSpeedLimitSchedule: "Suivre de nouveau la planification des limites de vitesse",
  ignoreSchedule: "Ignorer la planification",
  ignoreTheSpeedLimitSchedule:
    "Ignorer la planification des limites de vitesse jusqu’à son prochain changement",
  clickToChangeTheSchedule: "{scheduleTooltip}\nCliquez pour modifier la planification.",
  scheduleIgnoredShort: "Planification ignorée",
  scheduleIgnoredShortUntil: "Planification ignorée jusqu’à {time}",
  limitsUntilTime: "{limits} jusqu’à {time}",

  // components/LimitScheduleWeek
  speedLimitsThroughTheWeek: "Limites de vitesse au fil de la semaine",
  eachHourUploadsAboveDownloads:
    "Chaque heure\u00a0: les envois en haut, les téléchargements en bas",
  now: "Maintenant",
  limited: "Limité",
  paused: "En pause",
  nowSpan: "Maintenant, {span}",

  // lib/limitSchedule
  ruleNumber: "Règle {number}",
  kibPerSecond: "{kib}\u00a0KiB/s",
  unlimitedInASentence: "illimités",
  pausedInASentence: "en pause",
  uploadsAndDownloads: "envois et téléchargements\u00a0: {limit}",
  uploadsThenDownloads: "envois\u00a0: {upload}, téléchargements\u00a0: {download}",
  allDay: "Toute la journée",
  twentyFourHoursFrom: "24\u00a0heures à partir de {start}",
  midnight: "minuit",
  fromToTheNextDay: "{start} à {end} le lendemain",
  fromTo: "{start} à {end}",
  noRuleTheOrdinaryLimits: "Aucune règle, les limites habituelles",
  spanDescription: "{weekday} de {start} à {end}\u00a0: {source}, {limits}",
  weekdayAndTime: "{weekday} {time}",
  waitingForTheSpeedLimitSchedule: "En attente de la planification des limites de vitesse",
  waitingForTheSpeedLimitScheduleUntil:
    "En attente de la planification des limites de vitesse jusqu’à {time}",
  scheduleIgnored:
    "Planification des limites de vitesse ignorée\u00a0: «\u00a0{rule}\u00a0» est mise de côté et les limites habituelles s’appliquent, {limits}.",
  scheduleApplies:
    "Planification des limites de vitesse\u00a0: «\u00a0{rule}\u00a0» s’applique, {limits}.",
  scheduleIgnoredUntil:
    "Planification des limites de vitesse ignorée jusqu’à {time}\u00a0: «\u00a0{rule}\u00a0» est mise de côté et les limites habituelles s’appliquent, {limits}.",
  scheduleAppliesUntil:
    "Planification des limites de vitesse\u00a0: «\u00a0{rule}\u00a0» s’applique jusqu’à {time}, {limits}.",
} satisfies TranslatedArea<"limits">;
