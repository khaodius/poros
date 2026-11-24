import type { TranslatedArea } from "../catalog";

export const preview = {
  // components/PreviewImage
  fitInThePane: "Ajustar al panel",
  showAtActualSize: "Mostrar a tamaño real",

  // components/PreviewPane
  brokenLink: "Vínculo roto",
  notARegularFile: "No es un archivo normal",
  theFileThisLinkLeads: "El archivo al que lleva este vínculo no existe.",
  thereIsNothingInIt: "No contiene nada que mostrar.",
  nothingToPreview: "Nada que mostrar",
  nothingToPreviewDetail: "Mueve el cursor a un archivo o carpeta para verlo aquí.",
  preview: "Vista previa",
  folders: { one: "{count} carpeta", many: "{count} de carpetas", other: "{count} carpetas" },
  files: { one: "{count} archivo", many: "{count} de archivos", other: "{count} archivos" },
  theAccountSignedInHere:
    "La cuenta con la que se inició sesión aquí no tiene permiso para abrirlo.",
  itWasDeletedMovedOr: "Se eliminó, se movió o se le cambió el nombre después de listarlo.",
  connectionLost: "Se perdió la conexión",
  theVaultIsLocked: "La bóveda está bloqueada",
  theVaultIsLockedDetail: "Desbloquéala de nuevo para ver la vista previa de sus archivos.",
  thePreviewTriesAgainAs: "La vista previa vuelve a intentarlo en cuanto la conexión quede libre.",
  itStayedBusyForA: "Siguió ocupada durante un rato. Vuelve a intentarlo en un momento.",
  thePreviewWaitsRatherThan: "La vista previa espera para no ralentizar la transferencia.",
  couldNotLoadThePreview: "No se pudo cargar la vista previa",
  openInEditorF4: "Abrir en el editor (F4)",
  openInEditor: "Abrir en el editor",
  thisFileChangedAfterIt: "Este archivo cambió después de listarlo.",
  onlyTheBeginningIsShown: "Solo se muestra el principio: {shown} de {size}.",
  listedAs0Bytes: "Listado con 0 bytes",
  listedAs0BytesDetail:
    "Algunos servidores no indican el tamaño de los archivos que generan ellos mismos, así que puede que no esté vacío. Al leerlo se ve lo que contiene.",
  showPreview: "Mostrar vista previa",
  previewNotLoaded: "Vista previa no cargada",
  previewNotLoadedDetail:
    "Leer de este almacenamiento puede cobrarse, así que el archivo solo se lee cuando se pide.",
  emptyFile: "Archivo vacío",
  theServerSaysThereIs: "El servidor indica que está vacío.",
  thereIsNothingInIt2: "Está vacío.",
  tooLargeToPreview: "Demasiado grande para la vista previa",
  loadAnyway: "Cargar de todos modos",
  thisPictureCannotBeShown: "Esta imagen no se puede mostrar",
  thisPictureCannotBeShownDetail:
    "Puede que esté dañada o que sea de un tipo que este sistema no decodifica.",
  theFirstBytes: {
    one: "El primer byte:",
    many: "Los primeros {count} de bytes:",
    other: "Los primeros {count} bytes:",
  },
  folderNotListed: "Carpeta no listada",
  folderNotListedDetail:
    "Listar en este almacenamiento puede cobrarse, así que la carpeta solo se lista cuando se pide.",
  cryptomatorVault: "Bóveda de Cryptomator",
  cryptomatorVaultDetail: "Abre la carpeta para desbloquearla y ver sus archivos.",
  emptyFolder: "Carpeta vacía",
  items: { one: "{count} elemento", many: "{count} de elementos", other: "{count} elementos" },
  thumbnailsNotLoaded: "Miniaturas no cargadas",
  thumbnailsNotLoadedDetail:
    "Leer de este almacenamiento puede cobrarse, así que las imágenes solo se leen cuando se pide.",
  itemsSelected: {
    one: "{count} elemento seleccionado",
    many: "{count} de elementos seleccionados",
    other: "{count} elementos seleccionados",
  },
  notTextOrAPicture: "No es texto ni una imagen",
  noPermissionToReadFile: "No tienes permiso para leer este archivo",
  noPermissionToReadFolder: "No tienes permiso para leer esta carpeta",
  fileNoLongerThere: "Este archivo ya no existe",
  folderNoLongerThere: "Esta carpeta ya no existe",
  pictureHasPixelsShowsUpTo:
    "Esta imagen mide {dimensions} píxeles. Las imágenes de hasta {megapixels} megapíxeles se muestran solas.",
  pictureHasPixelsMoreThanShown:
    "Esta imagen mide {dimensions} píxeles, más de lo que puede mostrar una vista previa.",
  pictureIsSizeLoadsUpTo:
    "Esta imagen ocupa {size}. Las imágenes de hasta {limit} se cargan solas.",
  pictureIsSizeMoreThanLoaded:
    "Esta imagen ocupa {size}, más de lo que puede cargar una vista previa.",
  sizeWithBytes: {
    one: "{size} ({count} byte)",
    many: "{size} ({count} de bytes)",
    other: "{size} ({count} bytes)",
  },
  type: "Tipo",
  link: "Vínculo",
  toAFolder: "A una carpeta",
  toAFile: "A un archivo",
  modified: "Modificado",
  permissions: "Permisos",
  owner: "Propietario",
  contentsWithSize: "{contents} ({size})",
  imageOfFormat: "Imagen {format}",
  textInEncoding: "Texto, {encoding}",
  dimensions: "Dimensiones",
  contains: "Contiene",
  selected: "Seleccionado",
  totalSize: "Tamaño total",
  foldersRow: "Carpetas",
  notCountedInTheSize: "No se cuenta en el tamaño",

  // components/PreviewSettings
  largestPictureLoaded: "Imagen más grande que se carga",
  largestPictureLoadedHint:
    "Las imágenes más grandes muestran sus datos y un botón Cargar de todos modos.",
  textReadFromAFile: "Texto que se lee de un archivo",
  textReadFromAFileHint: "De un archivo más largo se muestra el principio.",
  thumbnailSize: "Tamaño de las miniaturas",
  whenTheCursorRestsOn: "Cuando el cursor se detiene en un archivo",
  onAClickOfShow: "Al hacer clic en Mostrar vista previa",
  filesOnThisComputerLoad: "Los archivos de este equipo se cargan",
  filesOnServersLoad: "Los archivos de servidores se cargan",
  sftpFtpAndWebdav: "SFTP, FTP y WebDAV.",
  filesOnStorageThatCharges: "Los archivos de almacenamiento que cobra por lectura se cargan",
  s3BackblazeB2GoogleDrive:
    "S3, Backblaze B2, Google Drive, OneDrive y Box, y las bóvedas guardadas en ellos.",

  // components/PreviewSplit
  resizePreview: "Cambiar el tamaño de la vista previa",
  dragToResizeDoubleClick: "Arrastra para cambiar el tamaño, haz doble clic para restablecerlo",

  // components/PreviewThumbnails
  andMore: { one: "y {count} más", many: "y {count} más", other: "y {count} más" },

  // lib/previewLoad
  theFileChangesAsIt: "El archivo cambia mientras se lee",

  // lib/previewSniff
  webp: "WebP",
  pdfDocument: "Documento PDF",
  gzipArchive: "Archivo comprimido gzip",
  xzArchive: "Archivo comprimido xz",
  zstandardArchive: "Archivo comprimido Zstandard",
  sevenZipArchive: "Archivo comprimido 7-Zip",
  rarArchive: "Archivo comprimido RAR",
  tarArchive: "Archivo tar",
  elfProgram: "Programa ELF",
  sqliteDatabase: "Base de datos SQLite",
  officeDocument: "Documento de Office",
  oggMedia: "Multimedia Ogg",
  flacAudio: "Audio FLAC",
  wavAudio: "Audio WAV",
  aviVideo: "Video AVI",
  matroskaOrWebmVideo: "Video Matroska o WebM",
  webFont: "Fuente web",
  webassemblyModule: "Módulo WebAssembly",
  tiffImage: "Imagen TIFF",
  photoshopImage: "Imagen de Photoshop",
  rpmPackage: "Paquete RPM",
  heifImage: "Imagen HEIF",
  mpeg4Audio: "Audio MPEG-4",
  windowsProgram: "Programa de Windows",
  bzip2Archive: "Archivo comprimido bzip2",
  mp3Audio: "Audio MP3",
  arArchive: "Archivo ar",
  javaClass: "Clase de Java",
  machOProgram: "Programa Mach-O",
  zipArchive: "Archivo comprimido ZIP",
  quicktimeVideo: "Video QuickTime",
  mpeg4Video: "Video MPEG-4",
  epubBook: "Libro EPUB",
  javaArchive: "Archivo comprimido Java",
  androidPackage: "Paquete de Android",

  // lib/previewText
  utf8WithBom: "UTF-8 con BOM",
  latin1: "Latin-1",
} satisfies TranslatedArea<"preview">;
