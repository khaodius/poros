import type { TranslatedArea } from "../catalog";

export const updates = {
  // components/SettingsDialog
  downloadingVersion: "Descargando {version}",
  downloadingVersionPercent: "Descargando {version}, {percent}",
  installingVersion: "Instalando {version}",
  checkingForUpdates: "Buscando actualizaciones",
  versionIsAvailable: "{version} está disponible",
  upToDateCheckedAt: "Actualizado, comprobado a las {time}",
  theVersionYouAreRunning: "La versión que estás usando",
  couldNotListTheReleases: "No se pudieron listar las versiones: {message}",
  noOtherReleaseToInstall: "No hay ninguna otra versión que instalar a partir de {version}.",
  switchToAnotherRelease: "Cambiar a otra versión a partir de {version}.",
  versionIsReadyRestart: "{version} está lista. Reinicia Poros para cambiar a ella.",
  restartingInto: "Reiniciando con {version}",
  couldNotInstallVersionBecause: "No se pudo instalar {version}: {message}",

  // components/StatusBar
  restartToUse: "Reinicia para usar {version}",
  restarting: "Reiniciando",
  couldNotInstallVersion: "No se pudo instalar {version}",

  // components/UpdateDialog
  updateAvailable: "Actualización disponible",
  isAvailableYouHave: "Poros {version} está disponible. Tienes la {current}.",
  porosKeepsWorkingWhileThe:
    "Poros sigue funcionando mientras se descarga la actualización y pregunta antes de reiniciarse.",
  skipThisVersion: "Omitir esta versión",
  later: "Más tarde",
  update: "Actualizar",

  // components/UpdateNotice
  hideTheStatusBarShows: "Ocultar; la barra de estado lo vuelve a mostrar",
  restartPorosToSwitchTo:
    "Reinicia Poros para cambiar ahora, o el cambio se aplicará la próxima vez que abras Poros.",
  restartNow: "Reiniciar ahora",
  of: "{received} de {total}",
  downloadingPoros: "Descargando Poros {version}",
  installingPoros: "Instalando Poros {version}",
  porosIsReady: "Poros {version} está listo",
  restartingPoros: "Reiniciando Poros",
  couldNotInstallPoros: "No se pudo instalar Poros {version}",

  // state/updateStore
  transfersWillStop: {
    one: "{count} transferencia de la cola se detendrá cuando Poros se reinicie.",
    many: "{count} de transferencias de la cola se detendrán cuando Poros se reinicie.",
    other: "{count} transferencias de la cola se detendrán cuando Poros se reinicie.",
  },
  transfersWillStopAndComeBackPaused: {
    one: "{count} transferencia de la cola se detendrá cuando Poros se reinicie y volverá en pausa.",
    many: "{count} de transferencias de la cola se detendrán cuando Poros se reinicie y volverán en pausa.",
    other:
      "{count} transferencias de la cola se detendrán cuando Poros se reinicie y volverán en pausa.",
  },
  unsavedChangesInFilesWill: {
    one: "Los cambios sin guardar de {count} archivo se perderán cuando Poros se reinicie.",
    many: "Los cambios sin guardar de {count} de archivos se perderán cuando Poros se reinicie.",
    other: "Los cambios sin guardar de {count} archivos se perderán cuando Poros se reinicie.",
  },
  theLatestReleaseDoesNotOfferInAppUpdates:
    "La última versión en GitHub todavía no ofrece actualizaciones desde la aplicación",
  couldNotCheckForUpdates: "No se pudo buscar actualizaciones: {message}",
  heldSavesUploadedAfterTheRestart: {
    one: "Un archivo abierto en otro programa tiene un guardado que aún no se ha subido. Poros ofrece subirlo después del reinicio.",
    many: "{count} de archivos abiertos en otros programas tienen guardados que aún no se han subido. Poros ofrece subirlos después del reinicio.",
    other:
      "{count} archivos abiertos en otros programas tienen guardados que aún no se han subido. Poros ofrece subirlos después del reinicio.",
  },
  heldSavesWipedByTheRestart: {
    one: "Un archivo abierto en otro programa tiene un guardado que aún no se ha subido. Al reiniciar se borra, porque los archivos de bóvedas no se conservan sin cifrar.",
    many: "{count} de archivos abiertos en otros programas tienen guardados que aún no se han subido. Al reiniciar se borran, porque los archivos de bóvedas no se conservan sin cifrar.",
    other:
      "{count} archivos abiertos en otros programas tienen guardados que aún no se han subido. Al reiniciar se borran, porque los archivos de bóvedas no se conservan sin cifrar.",
  },
  heldSavesPartlyWipedByTheRestart: {
    one: "Un archivo abierto en otro programa tiene un guardado que aún no se ha subido. Los de bóvedas se borran; Poros ofrece subir los demás después del reinicio.",
    many: "{count} de archivos abiertos en otros programas tienen guardados que aún no se han subido. Los de bóvedas se borran; Poros ofrece subir los demás después del reinicio.",
    other:
      "{count} archivos abiertos en otros programas tienen guardados que aún no se han subido. Los de bóvedas se borran; Poros ofrece subir los demás después del reinicio.",
  },
} satisfies TranslatedArea<"updates">;
