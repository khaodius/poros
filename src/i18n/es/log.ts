import type { TranslatedArea } from "../catalog";

export const log = {
  // components/LogView
  saveLog: "Guardar registro",
  levels: "Niveles",
  session: "Sesión",
  allSessions: "Todas las sesiones",
  searchLog: "Buscar en el registro",
  clearSearch: "Borrar búsqueda",
  showTimes: "Mostrar horas",
  wrapLongLines: "Ajustar las líneas largas",
  followNewLines: "Seguir las líneas nuevas",
  copyShownLines: "Copiar las líneas mostradas",
  saveShownLinesToA: "Guardar las líneas mostradas en un archivo",
  clearLog: "Borrar registro",
  info: "Información",
  warnings: "Advertencias",
  errors: "Errores",
  server: "Servidor",
  closedSession: "Sesión cerrada {id}",
  connectionAndTransferActivityAppears: "Aquí aparece la actividad de conexión y transferencia.",
  noLinesMatchTheFilters: "Ninguna línea coincide con los filtros.",
  logFiles: "Archivos de registro",
} satisfies TranslatedArea<"log">;
