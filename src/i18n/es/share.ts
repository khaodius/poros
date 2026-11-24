import type { TranslatedArea } from "../catalog";

export const share = {
  // components/ShareDialog
  share: "Compartir {name}",
  share2: "Compartir {selectionSummary}",
  onIn: "En {service}, en {folder}",
  links: "Enlaces",
  lookingForLinksItAlready: "Buscando los enlaces que ya tiene...",
  changeLink: "Cambiar enlace",
  newLink: "Nuevo enlace",
  letsPeople: "Las personas pueden",
  whoCanOpenIt: "Quién puede abrirlo",
  worksFor: "Válido durante",
  noEnd: "Sin caducidad",
  custom: "Personalizado",
  howMany: "Cuántos",
  unit: "Unidad",
  passwordOptional: "Contraseña (opcional)",
  anyoneWithTheLinkCan: "Cualquier persona con el enlace puede abrirlo",
  copyNewLink: "Copiar nuevo enlace",
  copyNewLinks: "Copiar nuevos enlaces",
  linkAddress: "Dirección del enlace",
  openInBrowser: "Abrir en el navegador",
  stopSharing: "Dejar de compartir",
  peopleWhoHaveThisLink: "Las personas que tengan este enlace ya no podrán abrirlo.",
  keepSharing: "Seguir compartiendo",
  minutes2: "Minutos",
  hours2: "Horas",
  days2: "Días",
  link: "Enlace",
  links2: { one: "{count} enlace", many: "{count} de enlaces", other: "{count} enlaces" },
  keepsOneLinkPerItem:
    "{service} mantiene un enlace por elemento, así que esto cambia el enlace de arriba para todos los que lo tienen.",
  keepsOneLinkPerItemSo:
    "{service} mantiene un enlace por elemento, así que a un elemento que ya tiene uno se le cambia para todos los que lo tienen.",
  changing: "Cambiando...",
  creating: "Creando...",
  copied: "Copiado",
  stopping: "Deteniendo...",
  couldNotReadTheLinksItHas: "No se pudieron leer los enlaces que tiene: {message}",
  createLink: "Crear enlace",
  createLinks: {
    one: "Crear {count} enlace",
    many: "Crear {count} de enlaces",
    other: "Crear {count} enlaces",
  },
  copyLink: "Copiar enlace",
  checking: "Comprobando...",
  makingTheLink: "Creando el enlace...",
  noLinkYet: "Aún no hay enlace",

  // lib/shareLinks
  shareLinkNotInA: "Enlace para compartir (no disponible en bóvedas)",
  shareLinkCloudStorageOnly: "Enlace para compartir (solo en la nube)",
  shareLinkFilesOnly: "Enlace para compartir (solo archivos)",
  shareLink: "Enlace para compartir...",
  days: { one: "{count} día", many: "{count} de días", other: "{count} días" },
  hours: { one: "{count} hora", many: "{count} de horas", other: "{count} horas" },
  minutes: { one: "{count} minuto", many: "{count} de minutos", other: "{count} minutos" },
  seconds: { one: "{count} segundo", many: "{count} de segundos", other: "{count} segundos" },
  files: { one: "{count} archivo", many: "{count} de archivos", other: "{count} archivos" },
  folders: { one: "{count} carpeta", many: "{count} de carpetas", other: "{count} carpetas" },
  view: "Ver",
  n1Hour: "1 hora",
  n1Day: "1 día",
  n7Days: "7 días",
  anyoneWithTheLink: "Cualquier persona con el enlace",
  peopleAlreadyInvited: "Personas ya invitadas",
  peopleIn: "Personas de {organization}",
  peopleInYourOrganization: "Personas de tu organización",
  enterHowLongTheLinkShouldWork:
    "Indica cuánto tiempo debe funcionar el enlace, con un número entero.",
  chooseHowLongTheLinkShouldWork: "Elige cuánto tiempo debe funcionar el enlace.",
  linksHereLastAtMost: "Aquí los enlaces duran como máximo {lifetime}.",
  anyoneWithThisLinkCanDownload:
    "Cualquier persona con este enlace puede descargar {what} {until}.",
  anyoneWithThisLinkCanEdit:
    "Cualquier persona con este enlace puede abrir y cambiar {what} {until}.",
  anyoneWithThisLinkCanView: "Cualquier persona con este enlace puede abrir {what} {until}.",
  anyoneWithOneOfTheseLinksCanDownload:
    "Cualquier persona con uno de estos enlaces puede descargar {what} {until}.",
  anyoneWithOneOfTheseLinksCanEdit:
    "Cualquier persona con uno de estos enlaces puede abrir y cambiar {what} {until}.",
  anyoneWithOneOfTheseLinksCanView:
    "Cualquier persona con uno de estos enlaces puede abrir {what} {until}.",
  peopleInWithThisLinkCanDownload:
    "Las personas de {organization} con este enlace pueden descargar {what} {until}.",
  peopleInWithThisLinkCanEdit:
    "Las personas de {organization} con este enlace pueden abrir y cambiar {what} {until}.",
  peopleInWithThisLinkCanView:
    "Las personas de {organization} con este enlace pueden abrir {what} {until}.",
  peopleInWithOneOfTheseLinksCanDownload:
    "Las personas de {organization} con uno de estos enlaces pueden descargar {what} {until}.",
  peopleInWithOneOfTheseLinksCanEdit:
    "Las personas de {organization} con uno de estos enlaces pueden abrir y cambiar {what} {until}.",
  peopleInWithOneOfTheseLinksCanView:
    "Las personas de {organization} con uno de estos enlaces pueden abrir {what} {until}.",
  peopleInYourOrganizationWithThisLinkCanDownload:
    "Las personas de tu organización con este enlace pueden descargar {what} {until}.",
  peopleInYourOrganizationWithThisLinkCanEdit:
    "Las personas de tu organización con este enlace pueden abrir y cambiar {what} {until}.",
  peopleInYourOrganizationWithThisLinkCanView:
    "Las personas de tu organización con este enlace pueden abrir {what} {until}.",
  peopleInYourOrganizationWithOneOfTheseLinksCanDownload:
    "Las personas de tu organización con uno de estos enlaces pueden descargar {what} {until}.",
  peopleInYourOrganizationWithOneOfTheseLinksCanEdit:
    "Las personas de tu organización con uno de estos enlaces pueden abrir y cambiar {what} {until}.",
  peopleInYourOrganizationWithOneOfTheseLinksCanView:
    "Las personas de tu organización con uno de estos enlaces pueden abrir {what} {until}.",
  peopleAlreadyInvitedWhoHaveThisLinkCanDownload:
    "Las personas ya invitadas que tengan este enlace pueden descargar {what} {until}.",
  peopleAlreadyInvitedWhoHaveThisLinkCanEdit:
    "Las personas ya invitadas que tengan este enlace pueden abrir y cambiar {what} {until}.",
  peopleAlreadyInvitedWhoHaveThisLinkCanView:
    "Las personas ya invitadas que tengan este enlace pueden abrir {what} {until}.",
  peopleAlreadyInvitedWhoHaveOneOfTheseLinksCanDownload:
    "Las personas ya invitadas que tengan uno de estos enlaces pueden descargar {what} {until}.",
  peopleAlreadyInvitedWhoHaveOneOfTheseLinksCanEdit:
    "Las personas ya invitadas que tengan uno de estos enlaces pueden abrir y cambiar {what} {until}.",
  peopleAlreadyInvitedWhoHaveOneOfTheseLinksCanView:
    "Las personas ya invitadas que tengan uno de estos enlaces pueden abrir {what} {until}.",
  statementWithPassword: "{statement} Primero pide la contraseña.",
  theItemItIsFor: "el elemento al que corresponde",
  theFileItIsFor: "el archivo al que corresponde",
  theFolderAndEverythingInIt: "la carpeta y todo su contenido",
  theFile: "el archivo",
  untilDeadline: "hasta el {deadline}",
  untilYouStopSharing: "hasta que dejes de compartir",
  forAsLongAsItIsThere: "mientras exista",
  sharedBefore: "Compartido antes: {statement}",
  notWhatWasAsked: "No es lo que se pidió: {statement}",
} satisfies TranslatedArea<"share">;
