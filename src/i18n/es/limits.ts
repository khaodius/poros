import type { TranslatedArea } from "../catalog";

export const limits = {
  // components/LimitScheduleSettings
  speedLimitSchedule: "Horario de límites de velocidad",
  followTheSchedule: "Seguir el horario",
  followTheScheduleHint:
    "Otros límites, o una pausa, a horas fijas de la semana según la hora de este equipo. Donde no se aplica ninguna regla, rigen los límites de arriba.",
  whereRulesOverlapTheOne:
    "Cuando las reglas se solapan, gana la que está más abajo en la lista. Una pausa detiene las transferencias en esa dirección, que continúan donde se quedaron cuando termina. La sincronización de carpetas, las actualizaciones con rsync y las tareas programadas también siguen el horario, igual que poros-cli, salvo que reciba sus propios límites.",
  addRule: "Agregar regla",
  theScheduleIsOffThe:
    "El horario está desactivado: la semana muestra lo que harían sus reglas una vez activado.",
  closeTheRule: "Cerrar la regla",
  editTheRule: "Editar la regla",
  moveUp: "Subir",
  moveUp2: "Subir {label}",
  moveDownSoItWins: "Bajar, para que gane a las reglas de arriba",
  moveDown: "Bajar {label}",
  deleteRule: "Eliminar regla",
  delete: "Eliminar {label}",
  nameOf: "Nombre de {label}",
  nameSuchAsOfficeHours: 'Nombre, como "Horario de oficina"',
  daysOfTheWeek: "Días de la semana",
  startTime: "Hora de inicio",
  to: "a",
  endTime: "Hora de fin",
  limitTo: "Limitar a",
  unlimited: "Sin límite",
  noDaysSoItNever: "Ningún día, así que nunca se aplica",
  uploadLimit: "Límite de subida",
  downloadLimit: "Límite de descarga",

  // components/LimitScheduleStatus
  followSchedule: "Seguir el horario",
  followTheSpeedLimitSchedule: "Volver a seguir el horario de límites de velocidad",
  ignoreSchedule: "Ignorar el horario",
  ignoreTheSpeedLimitSchedule: "Ignorar el horario de límites de velocidad hasta su próximo cambio",
  clickToChangeTheSchedule: "{scheduleTooltip}\nHaz clic para cambiar el horario.",
  scheduleIgnoredShort: "Horario ignorado",
  scheduleIgnoredShortUntil: "Horario ignorado hasta {time}",
  untilTime: "hasta {time}",

  // components/LimitScheduleWeek
  speedLimitsThroughTheWeek: "Límites de velocidad a lo largo de la semana",
  eachHourUploadsAboveDownloads: "Cada hora: subidas arriba, descargas abajo",
  now: "Ahora",
  limited: "Limitado",
  paused: "En pausa",
  nowSpan: "Ahora, {span}",

  // lib/limitSchedule
  ruleNumber: "Regla {number}",
  kibPerSecond: "{kib} KiB/s",
  unlimitedInASentence: "sin límite",
  pausedInASentence: "en pausa",
  uploadsAndDownloads: "subidas y descargas {limit}",
  uploadsThenDownloads: "subidas {upload}, descargas {download}",
  allDay: "Todo el día",
  twentyFourHoursFrom: "24 horas desde las {start}",
  midnight: "medianoche",
  fromToTheNextDay: "De {start} a {end} del día siguiente",
  fromTo: "De {start} a {end}",
  noRuleTheOrdinaryLimits: "Ninguna regla, los límites normales",
  spanDescription: "{weekday} de {start} a {end}: {source}, {limits}",
  weekdayAndTime: "{weekday} {time}",
  waitingForTheSpeedLimitSchedule: "Esperando al horario de límites de velocidad",
  waitingForTheSpeedLimitScheduleUntil: "Esperando al horario de límites de velocidad hasta {time}",
  scheduleIgnored:
    'Horario de límites de velocidad ignorado: "{rule}" queda en suspenso y se aplican los límites normales, {limits}.',
  scheduleApplies: 'Horario de límites de velocidad: se aplica "{rule}", {limits}.',
  scheduleIgnoredUntil:
    'Horario de límites de velocidad ignorado hasta {time}: "{rule}" queda en suspenso y se aplican los límites normales, {limits}.',
  scheduleAppliesUntil:
    'Horario de límites de velocidad: se aplica "{rule}" hasta {time}, {limits}.',
} satisfies TranslatedArea<"limits">;
