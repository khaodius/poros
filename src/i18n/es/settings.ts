import type { TranslatedArea } from "../catalog";

export const settings = {
  // components/LanguagePicker
  languageSystem: "Sistema ({language})",

  // components/SettingsDialog
  settingsSections: "Secciones de la configuración",
  queue: "Cola",
  simultaneousTransfers: "Transferencias simultáneas",
  simultaneousTransfersHint: "Cuántos archivos se mueven a la vez.",
  startTransfersAsSoonAs: "Iniciar las transferencias en cuanto entren en la cola",
  startTransfersAsSoonAsHint: "Desactivado: la cola espera hasta que presiones Iniciar cola.",
  whenTheTargetFileExists: "Cuando el archivo de destino existe",
  keepFinishedTransfersInThe: "Conservar en la lista las transferencias terminadas",
  keepUnfinishedTransfersWhenPoros: "Conservar las transferencias sin terminar al cerrar Poros",
  keepUnfinishedTransfersWhenPorosHint:
    "Vuelven en pausa en el siguiente inicio. Las contraseñas nunca se guardan con ellas.",
  connections: "Conexiones",
  giveEachTransferItsOwn: "Dar a cada transferencia su propia conexión",
  giveEachTransferItsOwnHint:
    "Desactivado: las transferencias comparten la conexión de navegación, para servidores que limitan los inicios de sesión.",
  splitLargeFilesAcrossConnections: "Dividir los archivos grandes entre varias conexiones",
  splitLargeFilesAcrossConnectionsHint:
    "Las ranuras de transferencia libres se suman a un archivo grande y mueven partes distintas de él.",
  splitFilesLargerThan: "Dividir los archivos de más de",
  connectionsPerFile: "Conexiones por archivo",
  copyDirectlyBetweenFtpServers: "Copiar directamente entre servidores FTP (FXP)",
  copyDirectlyBetweenFtpServersHint:
    "Los dos servidores deben permitirlo. Si no, y entre otros servidores, los archivos pasan por Poros.",
  bandwidth: "Ancho de banda",
  uploadLimit: "Límite de subida",
  uploadLimitHint: "0 significa sin límite. Lo comparten todas las transferencias.",
  downloadLimit: "Límite de descarga",
  buffers: "Búferes",
  requestSize: "Tamaño de las solicitudes",
  requestSizeHint:
    "Bytes por lectura o escritura SFTP. La mayoría de los servidores aceptan hasta 255 KiB; algunos limitan a 32.",
  requestsInFlight: "Solicitudes en curso",
  requestsInFlightHint:
    "Solicitudes pendientes por conexión. Un valor mayor oculta más latencia de red.",
  files: "Archivos",
  keepModificationTimes: "Conservar las horas de modificación",
  keepPermissions: "Conservar los permisos",
  keepPermissionsHint:
    "Copia los bits de permisos de Unix. No tiene efecto en archivos de Windows.",
  integrity: "Integridad",
  writeToATemporaryName: "Escribir con un nombre temporal y luego cambiar el nombre",
  writeToATemporaryNameHint:
    "Un archivo solo aparece con su nombre cuando está completo y comprobado, y un archivo reemplazado nunca queda a medio escribir. Las subidas a servidores FTP se escriben directamente.",
  verifyChecksums: "Verificar sumas de comprobación",
  verifyChecksumsHint:
    "Compara un SHA-256 de las dos copias después de cada archivo, que en los servidores SFTP calcula su propio sha256sum. Nunca se usan SHA-1 ni MD5 para esto, porque se pueden falsificar. Lee cada archivo una vez más en los dos lados.",
  readFilesBackWhenThe: "Volver a leer los archivos cuando el servidor no puede calcular su hash",
  readFilesBackWhenTheHint:
    "En los servidores SFTP sin sha256sum, y en los servidores FTP y WebDAV, cada archivo terminado se vuelve a leer y se calcula su hash aquí. Duplica el tráfico con esos servidores. El almacenamiento en la nube no se vuelve a leer.",
  repairOnlyTheDamagedParts: "Reparar solo las partes dañadas",
  repairOnlyTheDamagedPartsHint:
    "Tras una discrepancia, cada parte del archivo se compara por separado y solo se vuelven a enviar las partes que difieren, hasta la mitad del archivo. Necesita GNU split en el servidor, o volver a leer.",
  checkTheTransferredPartBefore: "Comprobar la parte transferida antes de reanudar",
  checkTheTransferredPartBeforeHint:
    "Compara lo que ya hay con el origen, parte por parte, y continúa desde la primera parte que difiere. Donde el servidor no puede calcular el hash de partes de un archivo, se comparan los últimos 64 KiB.",
  flushToDiskBeforeFinishing: "Volcar al disco antes de terminar",
  flushToDiskBeforeFinishingHint:
    "Un archivo solo cuenta como terminado una vez almacenado, así que un corte de luz justo después no puede hacer que se pierda. En los servidores que lo admiten; ralentiza los lotes de archivos pequeños.",
  checkTimesAndPermissionsAfter: "Comprobar las horas y los permisos después de establecerlos",
  checkTimesAndPermissionsAfterHint:
    "Los vuelve a leer cuando el archivo está en su sitio y anota en el registro los que el servidor o el sistema de archivos no conservaron.",
  checkPublishedChecksumsAndSignatures: "Comprobar las sumas de comprobación y firmas publicadas",
  checkPublishedChecksumsAndSignaturesHint:
    "Las descargas se comparan con SHA256SUMS, name.sha256 y archivos similares que haya junto a ellas, con firmas .minisig y, mediante gpg, con firmas .asc y .sig. Un archivo que no supera la comprobación no se guarda.",
  errors: "Errores",
  retries: "Reintentos",
  retriesHint:
    "Para tiempos de espera agotados, comprobaciones fallidas y conexiones que se vuelven a caer enseguida.",
  waitBetweenRetries: "Espera entre reintentos",
  trustedMinisignKeys: "Claves de minisign de confianza",
  onePublicKeyPerLine:
    "Una clave pública por línea, como en el minisign.pub de un editor. Las firmas de otras claves se anotan en el registro y no se comprueban.",
  autoTuneReceiveBuffer: "Ajustar automáticamente el búfer de recepción",
  autoTuneReceiveBufferHint:
    "El sistema amplía la ventana de recepción TCP según la conexión. Se aplica a las conexiones nuevas.",
  receiveBufferSize: "Tamaño del búfer de recepción",
  receiveBufferSizeHint: "SO_RCVBUF, que se usa cuando el ajuste automático está desactivado.",
  autoTuneSendBuffer: "Ajustar automáticamente el búfer de envío",
  autoTuneSendBufferHint:
    "El sistema dimensiona el búfer de envío TCP según la conexión. Se aplica a las conexiones nuevas.",
  sendBufferSize: "Tamaño del búfer de envío",
  sendBufferSizeHint: "SO_SNDBUF, que se usa cuando el ajuste automático está desactivado.",
  connectionTimeout: "Tiempo de espera de conexión",
  keepaliveInterval: "Intervalo de keepalive",
  keepaliveIntervalHint: "0 desactiva los keepalive. Se aplica a las conexiones nuevas.",
  compressTraffic: "Comprimir el tráfico",
  compressTrafficHint:
    "Ayuda con el texto en enlaces lentos; ralentiza los archivos ya comprimidos. Se aplica a las conexiones nuevas.",
  reconnecting: "Reconexión",
  reconnectLostTabsAutomatically: "Reconectar automáticamente las pestañas perdidas",
  reconnectLostTabsAutomaticallyHint:
    "Lo vuelve a intentar con esperas cada vez mayores, sin volver a pedir las contraseñas ya dadas.",
  keepTryingFor: "Seguir intentándolo durante",
  keepTryingForHint:
    "Las pestañas y las transferencias esperan este tiempo a un servidor perdido; mientras tanto, las transferencias conservan sus reintentos. 0 se rinde de inmediato.",
  savedConnections: "Conexiones guardadas",
  saveQuickConnectionsAutomatically: "Guardar automáticamente las conexiones rápidas",
  saveQuickConnectionsAutomaticallyHint:
    "Agrega cada servidor nuevo al que llegues desde la barra de conexión rápida, sin su contraseña.",
  proxy: "Proxy",
  newSftpConnectionsReachTheir:
    "Las conexiones SFTP nuevas llegan a su servidor a través de este proxy, salvo que una conexión esté configurada para conectarse sin él. Con un host de salto, solo la conexión al host de salto usa el proxy. Las conexiones FTP y en la nube no lo usan.",
  type: "Tipo",
  host: "Host",
  port: "Puerto",
  username: "Nombre de usuario",
  usernameHint:
    "Déjalo vacío para un proxy sin inicio de sesión. SOCKS4 solo envía el nombre de usuario.",
  password: "Contraseña",
  passwordHint: "Se guarda en el llavero del sistema.",
  forget: "Olvidar",
  lookUpServerNamesThrough: "Resolver los nombres de servidor a través del proxy",
  lookUpServerNamesThroughHint:
    "El DNS de este equipo nunca ve los nombres de los servidores a los que te conectas. Los proxies HTTP siempre lo hacen así.",
  proxyPassword: "Contraseña del proxy",
  saved: "Guardada",
  doubleClickingAFile: "Doble clic en un archivo",
  transfersItToTheOther: "Lo transfiere al otro lado",
  opensItInTheBuilt: "Lo abre en el editor integrado",
  opensItInItsProgram: "Lo abre en su programa",
  asksHowToOpenIt: "Pregunta cómo abrirlo",
  doesNothing: "No hace nada",
  showHiddenFilesInNew: "Mostrar archivos ocultos en las pestañas nuevas",
  foldersFirst: "Carpetas primero",
  foldersFirstHint: "Mantiene las carpetas encima de los archivos sea cual sea el orden.",
  dates: "Fechas",
  columns: "Columnas",
  columnsHint:
    "Haz clic con el botón derecho en un encabezado de columna para mostrar u ocultar columnas y cambiar el orden. Arrastra la línea anterior a un encabezado para cambiar el tamaño de su columna, o haz doble clic en la línea para ajustarla.",
  serverFolders: "Carpetas del servidor",
  showCachedListingsWhileRefreshing: "Mostrar listados en caché mientras se actualizan",
  showCachedListingsWhileRefreshingHint:
    "Una carpeta abierta antes se muestra al instante tal como se vio la última vez y se actualiza cuando responde el servidor. Los listados solo se guardan en memoria, nunca en el disco.",
  loadNearbyFoldersAhead: "Cargar por adelantado las carpetas cercanas",
  loadNearbyFoldersAheadHint:
    "Lista la carpeta superior y la carpeta bajo el cursor mientras el servidor está inactivo, de una en una. En pausa en FTP mientras hay transferencias.",
  alsoOnCloudStorage: "También en el almacenamiento en la nube",
  alsoOnCloudStorageHint:
    "Desactivado: no se lista nada por adelantado en el almacenamiento en la nube, donde las solicitudes se cuentan y se limitan.",
  tabsAndWindows: "Pestañas y ventanas",
  askBeforeClosingATab: "Preguntar antes de cerrar una pestaña con transferencias en cola",
  rememberTabsAndLayout: "Recordar las pestañas y el diseño",
  rememberTabsAndLayoutHint:
    "Vuelve a abrir las ventanas, paneles y pestañas abiertos al salir la última vez, cada uno en su carpeta.",
  reconnectToServersOnStart: "Reconectar a los servidores al iniciar",
  reconnectToServersOnStartHint:
    "Desactivado: las pestañas de servidor reabiertas esperan a que las conectes.",
  useTheSystemTitleBar: "Usar la barra de título del sistema",
  useTheSystemTitleBarHint:
    "Desactivado: la barra superior contiene los botones de la ventana y sirve para arrastrarla.",
  log: "Registro",
  linesToKeep: "Líneas que se conservan",
  showTimes: "Mostrar horas",
  wrapLongLines: "Ajustar las líneas largas",
  logEveryTransferredFile: "Registrar cada archivo transferido",
  logEveryTransferredFileHint: "Desactivado: una línea de resumen al terminar la cola.",
  updates: "Actualizaciones",
  checkForUpdatesWhenPoros: "Buscar actualizaciones al iniciar Poros",
  checkForUpdatesWhenPorosHint:
    "Busca una versión más reciente en GitHub. No se descarga nada hasta que decidas instalarla.",
  poros: "Poros {version}",
  restartNow: "Reiniciar ahora",
  updating: "Actualizando...",
  showUpdate: "Mostrar actualización",
  isNotAvailable: "{version} no está disponible: {message}",
  installAnotherVersion: "Instalar otra versión",
  version: "Versión",
  install: "Instalar",
  transfers: "Transferencias",
  sync: "Sincronización",
  connection: "Conexión",
  cloudAccounts: "Cuentas en la nube",
  automation: "Automatización",
  archives: "Archivos comprimidos",
  interface: "Interfaz",
  appearance: "Apariencia",
  overwriteIfTheSourceIs: "Sobrescribir si el origen es más reciente",
  overwriteIfSizeOrDate: "Sobrescribir si difieren el tamaño o la fecha",
  keepBoth: "Conservar ambos",
  checking: "Comprobando...",
  checkForUpdates: "Buscar actualizaciones",
  listing: "Listando...",
  showVersions: "Mostrar versiones",
  language: "Idioma",
  languageHint:
    "Sistema sigue el idioma del equipo. Los mensajes de los servidores y el registro quedan en inglés.",

  // components/Stepper
  decrease: "Disminuir {label}",
  increase: "Aumentar {label}",
} satisfies TranslatedArea<"settings">;
