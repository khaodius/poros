import type { TranslatedArea } from "../catalog";

export const connect = {
  // components/ConnectDialog
  selectPrivateKey: "Seleccionar clave privada",
  savedConnections: "Conexiones guardadas",
  importFromFilezillaWinscpAnd: "Importar de FileZilla, WinSCP y otras aplicaciones",
  importConnections: "Importar conexiones",
  newConnection: "Nueva conexión",
  connectionsYouSaveAppearHere: "Las conexiones que guardes aparecen aquí.",
  importThemFromAnotherApp: "Importarlas de otra aplicación",
  protocol: "Protocolo",
  account: "Cuenta",
  emptyForAmazonS3: "Vacío para Amazon S3",
  port: "Puerto",
  forAnotherServiceEnterIts:
    "Para otro servicio, escribe su punto de conexión, como {example}, o {local} para un servidor sin TLS.",
  anonymous: "anonymous",
  emptyIfTheServerNeeds: "Vacío si el servidor no lo pide",
  authentication: "Autenticación",
  savedInTheSystemKeychain: "Guardada en el llavero del sistema",
  privateKeyFile: "Archivo de clave privada",
  sshIdEd25519OrC: "~/.ssh/id_ed25519 o C:\\keys\\server.ppk",
  opensshPemPkcs8And: "Se admiten claves OpenSSH, PEM, PKCS#8 y PuTTY .ppk (v2 y v3).",
  usesKeysLoadedInYour:
    "Usa las claves cargadas en tu agente SSH: {variable} en Linux y macOS, el servicio OpenSSH Authentication Agent o Pageant en Windows.",
  region: "Región",
  fromTheEndpointOrUs: "Del punto de conexión, o us-east-1",
  bucketAddress: "Dirección del bucket",
  automatic: "Automática",
  pathEndpointBucket: "Ruta (punto de conexión/bucket)",
  virtualHostedBucketEndpoint: "Host virtual (bucket.punto de conexión)",
  remoteFolderOptional: "Carpeta remota (opcional)",
  jumpHost: "Host de salto",
  noneConnectStraightToThe: "Ninguno, conectar directamente al servidor",
  aDeletedConnection: "Una conexión eliminada",
  logsInToThisSaved:
    "Inicia sesión primero en esta conexión guardada y llega al servidor a través de ella, como ProxyJump en OpenSSH.",
  rememberThePassword: "Recordar la contraseña en el llavero del sistema",
  rememberThePassphrase: "Recordar la frase de contraseña en el llavero del sistema",
  rememberTheSecretAccessKey: "Recordar la clave de acceso secreta en el llavero del sistema",
  rememberTheApplicationKey: "Recordar la clave de aplicación en el llavero del sistema",
  activeModeTheServerOpens:
    "Modo activo: el servidor abre las conexiones de datos hacia este equipo",
  connectWithoutTheProxy: "Conectar sin el proxy",
  rsyncAndSynchronization: "rsync y sincronización",
  rsyncOnTheServer: "rsync en el servidor",
  theCommandThatStartsRsync:
    "El comando que inicia rsync aquí, como sudo rsync o /usr/local/bin/rsync. En blanco, se usa el de la configuración de transferencias.",
  rsyncArgumentsForFolderSynchronizations: "Argumentos de rsync para sincronizar carpetas",
  rsyncArgumentsForFolderSynchronizationsHint:
    "Las sincronizaciones con este servidor parten de los valores predeterminados de Configuración, con estos aplicados encima.",
  password: "Contraseña",
  keyFile: "Archivo de clave",
  sshAgent: "Agente SSH",
  host: "Host",
  username: "Nombre de usuario",
  browserSignIn: "Inicio de sesión en el navegador",
  endpoint: "Punto de conexión",
  saveThisConnectionAndIts: "Guardar esta conexión y su inicio de sesión",
  saveThisConnection: "Guardar esta conexión",
  deleteForGood: "Eliminar definitivamente",
  saving: "Guardando...",
  connecting: "Conectando...",
  shownInSavedConnections: "Se muestra en las conexiones guardadas",
  topFolderOfTheDrive: "Carpeta raíz de la unidad",
  bucketOrBucketFolder: "{bucket} o {folder}",
  loginFolder: "Carpeta de inicio de sesión",
  theFolderInTheAddressOrTheTop: "La carpeta de la dirección, o la raíz",
  homeFolder: "Carpeta personal",
  passphrase: "Frase de contraseña",
  passphraseIfTheKeyHasOne: "Frase de contraseña (si la clave tiene una)",
  hostOrAddress: "{host} o {address}",

  // components/HostKeyDialog
  theKeyPresentedBy:
    "La clave que presenta {target} no coincide con la registrada. Puede que el servidor se haya reinstalado o que alguien esté interceptando la conexión. Continúa solo si sabes por qué cambió la clave.",
  hasNotBeenSeenBefore:
    "{target} no se ha visto antes. Antes de continuar, compara la huella digital con la que te dio el administrador del servidor.",
  fingerprint: "Huella digital",
  connectOnce: "Conectar una vez",
  theCertificatePresentedBy:
    "El certificado que presenta {target} no coincide con el que aceptaste antes. {problem} Antes de continuar, compara la huella digital con la que te dio el administrador del servidor.",
  thisComputerDoesNotTrust:
    "Este equipo no confía en el certificado que presenta {target}. {problem} Antes de continuar, compara la huella digital con la que te dio el administrador del servidor.",
  type: "Tipo",
  keyType: "Tipo de clave",
  trustAndConnect: "Confiar y conectar",
  certificateChanged: "Certificado modificado",
  untrustedCertificate: "Certificado no confiable",
  hostKeyChanged: "Clave de host modificada",
  unknownHost: "Host desconocido",
  replaceCertificateAndConnect: "Reemplazar certificado y conectar",
  replaceKeyAndConnect: "Reemplazar clave y conectar",

  // components/ImportConnectionsDialog
  importFrom: "Importar de {label}",
  importedFrom: "Importación desde {label} completada: {imported}.",
  imported: "Importación completada: {imported}.",
  theseCouldNotBeSaved: "No se pudieron guardar estas: {failures}",
  importedConnections: {
    one: "{count} conexión",
    many: "{count} de conexiones",
    other: "{count} conexiones",
  },
  passwordsLeftOut: {
    one: "El llavero del sistema no está disponible, así que se omitió {count} contraseña; Poros la pedirá al conectar.",
    many: "El llavero del sistema no está disponible, así que se omitieron {count} de contraseñas; Poros las pedirá al conectar.",
    other:
      "El llavero del sistema no está disponible, así que se omitieron {count} contraseñas; Poros las pedirá al conectar.",
  },
  porosWorksOutWhichApp: "Poros averigua qué aplicación escribió el archivo",
  lookingForOtherApps: "Buscando otras aplicaciones...",
  porosCopiesTheSavedSites:
    "Poros copia los sitios guardados de estas aplicaciones en sus propias conexiones guardadas. En la otra aplicación no cambia nada.",
  chooseFile: "Elegir archivo...",
  alreadySaved: "Ya guardada",
  leftOutSincePorosCannot:
    "Omitidos: {count}, ya que Poros no puede conectarse a ellos tal como están",
  putFolderNamesInFront: "Anteponer los nombres de carpeta a los de conexión",
  reading: "Leyendo...",
  otherFile: "Otro archivo...",
  importing: "Importando...",
  import: "Importar",
  unnamed: "Sin nombre",
  noSavedSitesIn: "No hay sitios guardados en {origins}.",
  noSavedSitesInThatFile: "No hay sitios guardados en ese archivo.",
  importCountConnections: {
    one: "Importar {count} conexión",
    many: "Importar {count} de conexiones",
    other: "Importar {count} conexiones",
  },
  foundFiles: "Encontrado: {files}",
  notFoundInChoose: "No se encontró en {places}. {choose}",
  placeOrPlace: "{first} o {second}",
  keepThePasswordsInTheKeychain: {
    one: "Guardar la contraseña en el llavero del sistema",
    many: "Guardar los {count} de contraseñas en el llavero del sistema",
    other: "Guardar las {count} contraseñas en el llavero del sistema",
  },
  sitesFrom: {
    one: "{count} sitio de {source}",
    many: "{count} de sitios de {source}",
    other: "{count} sitios de {source}",
  },

  // components/QuickConnectBar
  cloudStorage: "Almacenamiento en la nube",
  hostOrUserHostPort: "Host o usuario@host:puerto",

  // components/SaveConnectionDialog
  saveConnection: "Guardar conexión",

  // lib/connectionImport
  filezilla: "FileZilla",
  winscp: "WinSCP",
  smartftp: "SmartFTP",
  cyberduckAndMountainDuck: "Cyberduck y Mountain Duck",
  puttyAndKitty: "PuTTY y KiTTY",
  mobaxterm: "MobaXterm",
  opensshConfig: "Configuración de OpenSSH",
  gnomeFilesAndKdeDolphin: "Archivos de GNOME y Dolphin de KDE",
  listOfAddresses: "Lista de direcciones",
  chooseFileZillaFiles:
    "Elige sitemanager.xml o un archivo exportado desde el Gestor de sitios (Archivo > Exportar).",
  chooseWinScpFiles: "Elige WinSCP.ini o un archivo de Tools > Export/Backup Configuration.",
  chooseSmartFtpFiles:
    "Elige la lista de Favorites > Tools > Export, o los favoritos .xml de versiones anteriores.",
  chooseCyberduckFiles: "Elige sus archivos de marcadores (.duck).",
  choosePuttyFiles:
    "Elige una exportación .reg de su clave de registro Sessions, o los archivos de sesión de PuTTY.",
  chooseMobaXtermFiles: "Elige MobaXterm.ini o un archivo .mxtsessions de Sessions > Export.",
  chooseOpenSshFiles: "Elige un archivo de configuración de ssh, como ~/.ssh/config.",
  chooseFileManagerFiles: "Elige gtk-3.0/bookmarks o user-places.xbel.",
  chooseUrlListFiles: "Elige un archivo de texto con una dirección sftp:// o ftp:// por línea.",

  // lib/protocols
  ftpsExplicitTls: "FTPS (TLS explícito)",
  ftpsImplicitTls: "FTPS (TLS implícito)",
  ftpsImplicit: "FTPS implícito",
  webdavHttp: "WebDAV (HTTP)",
  webdav: "WebDAV",
  webdavHttps: "WebDAV (HTTPS)",
  webdavs: "WebDAVS",
  s3AndCompatible: "S3 y compatibles",
  backblazeB2: "Backblaze B2",
  googleDrive: "Google Drive",
  onedrive: "OneDrive",
  box: "Box",
  cryptomatorVault: "Bóveda de Cryptomator",
  vault: "Bóveda",
  google: "Google",
  microsoft: "Microsoft",
  accessKeyId: "ID de clave de acceso",
  secretAccessKey: "Clave de acceso secreta",
  applicationKeyId: "ID de clave de aplicación",
  applicationKey: "Clave de aplicación",

  // state/connectFlow
  theHostKeyWasNot: "No se aceptó la clave de host.",

  // state/sessionStore
  itsTabWasClosedMeanwhile: "Mientras tanto, se cerró su pestaña.",
} satisfies TranslatedArea<"connect">;
