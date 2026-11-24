import type { TranslatedArea } from "../catalog";

export const workspace = {
  // App
  connectToServer: "Conectar a un servidor",
  synchronizeFolders: "Sincronizar carpetas",
  scheduledTasks: "Tareas programadas",
  settingsCtrl: "Configuración (Ctrl+,)",

  // components/BottomPanel
  queue: "Cola",
  failed: "Con error",
  completed: "Terminadas",
  startQueue: "Iniciar cola",
  pauseQueue: "Pausar cola",
  pauseSelected: "Pausar selección",
  resumeSelected: "Reanudar selección",
  moveSelectedToTheTop: "Mover la selección al principio",
  moveSelectedToTheBottom: "Mover la selección al final",
  removeSelected: "Quitar selección",
  retryAll: "Reintentar todo",
  retrySelected: "Reintentar selección",
  clearFailedTransfers: "Borrar las transferencias con error",
  keepEachTransferInView: "Mantener a la vista cada transferencia al terminar",
  follow: "Seguir",
  clearFinishedTransfers: "Borrar las transferencias terminadas",
  simultaneousTransfers: "Transferencias simultáneas",
  workers: "Simultáneas",
  log: "Registro",

  // components/CloseTabDialog
  close: "¿Cerrar {label}?",
  transfersIntoOrOutOf:
    "Las transferencias hacia o desde una bóveda desbloqueada se pausan en cualquier caso: al cerrar la pestaña se bloquea la bóveda. Reanúdalas cuando la bóveda vuelva a estar desbloqueada.",
  pauseTransfersAndClose: "Pausar transferencias y cerrar",
  keepTransferring: "Seguir transfiriendo",
  transfersForThisServerStillQueued: {
    one: "{count} transferencia de este servidor sigue en la cola. Puede seguir con su propia conexión después de cerrar la pestaña, o esperar en pausa hasta que la reanudes.",
    many: "{count} de transferencias de este servidor siguen en la cola. Pueden seguir con sus propias conexiones después de cerrar la pestaña, o esperar en pausa hasta que las reanudes.",
    other:
      "{count} transferencias de este servidor siguen en la cola. Pueden seguir con sus propias conexiones después de cerrar la pestaña, o esperar en pausa hasta que las reanudes.",
  },
  unmountAndClose: "Desmontar y cerrar",

  // components/DockArea
  dragToResizeDoubleClick: "Arrastra para cambiar el tamaño, haz doble clic para igualar",
  moveToMainWindow: "Mover a la ventana principal",
  newWindow: "Nueva ventana",
  moveToThatWindow: "Mover a esa ventana",

  // components/DragGhost
  uploadTo: "Subir a {folder}",
  downloadTo: "Descargar en {folder}",
  copyTo: "Copiar a {folder}",
  copyTo2: "Copiar a {baseName}",
  moveTo: "Mover a {baseName}",

  // components/DropMarker
  splitRight: "Dividir a la derecha",
  splitDown: "Dividir hacia abajo",
  mergeAsTab: "Combinar como pestaña",
  splitLeft: "Dividir a la izquierda",
  splitUp: "Dividir hacia arriba",

  // components/RestoringPane
  signIn: "Iniciar sesión",
  closeTab: "Cerrar pestaña",
  reopensNameOnceConnected: "Vuelve a abrir {name} al conectar.",
  opensANewShellOnceConnected: "Abre un shell nuevo al conectar.",
  reopensOnceConnected: "Se vuelve a abrir al conectar.",
  openingName: "Abriendo {name}...",
  openingFile: "Abriendo archivo...",
  reconnectingTo: "Reconectando con {label}...",
  signInTo: "Iniciar sesión en {label}",
  couldNotReconnectTo: "No se pudo reconectar con {label}",
  porosKeepsNoPasswordForThisServer: "Poros no guarda ninguna contraseña para este servidor.",

  // components/SplashScreen
  loadingPoros: "Cargando Poros",

  // components/StatusBar
  connectionToLost: "Se perdió la conexión con {label}",
  connectedTo: "Conectado a {label}",
  serversConnected: {
    one: "{count} servidor conectado",
    many: "{count} de servidores conectados",
    other: "{count} servidores conectados",
  },
  notConnected: "Sin conexión",
  updateTo: "Actualizar a {version}",
  uploadSpeed: "Velocidad de subida",
  downloadSpeed: "Velocidad de descarga",
  appVersion: "Poros {version}",
  transfersLeft: {
    one: "Queda {count} transferencia",
    many: "Quedan {count} de transferencias",
    other: "Quedan {count} transferencias",
  },
  aboutDuration: "cerca de {duration}",
  queueStatusPaused: "{status} (en pausa)",

  // components/TabBar
  openTerminal: "Abrir terminal",
  moveToNewWindow: "Mover a una ventana nueva",
  closeOtherTabs: "Cerrar las demás pestañas",
  newTabCtrlT: "Nueva pestaña (Ctrl+T)",
  newTab: "Nueva pestaña",
  disconnected: "Desconectado",
  unsavedChanges: "Cambios sin guardar",
  close2: "Cerrar {label}",
  closeCtrlW: "Cerrar (Ctrl+W)",
  mountAsDrive: "Montar como unidad...",
  mountedAsADriveAt: "Montado como unidad en {paths}",
  mountedAsADrive: "Montado como unidad",

  // components/TabContent
  local: "Local",
  thisSessionHasEnded: "Esta sesión ha terminado",
  connectionLost: "Se perdió la conexión",
  reconnectingInS: "Reconectando en {secondsLeft} s",
  reconnecting: "Reconectando...",
  reconnectingInSecondsAttempt: "Reconectando en {seconds} s (intento {attempt})",
  reconnectingAttempt: "Reconectando... (intento {attempt})",
  reconnectNow: "Reconectar ahora",
  reconnect: "Reconectar",

  // components/WelcomePanel
  localFiles: "Archivos locales",
  import: "Importar",
  importFromFilezillaWinscpAnd: "Importar de FileZilla, WinSCP y otras aplicaciones",
  savedConnections: "Conexiones guardadas",
  serversYouSaveFromThe:
    "Aquí aparecen los servidores que guardes desde el cuadro de conexión o desde el botón de marcador de una pestaña de servidor. Se pueden importar los sitios guardados en FileZilla, WinSCP, PuTTY y otras aplicaciones.",
  connecting: "Conectando...",

  // components/WindowControls
  minimize: "Minimizar",
  restore: "Restaurar",
  maximize: "Maximizar",

  // components/Workspace
  dragToResizeDoubleClickTo: "Arrastra para cambiar el tamaño, haz doble clic para restablecerlo",

  // hooks/useTabLabel
  closedSession: "Sesión cerrada",

  // state/restoreActions
  couldNotReopen: "No se pudo volver a abrir {name}: {message}",
  porosNoLongerKnowsHow: "Poros ya no sabe cómo llegar a él.",
} satisfies TranslatedArea<"workspace">;
