import type { TranslatedArea } from "../catalog";

export const archives = {
  // components/ArchiveSettings
  theBestOneInstalledFor: "El mejor instalado para cada formato",
  notFound: "{picked} (no encontrado)",
  anotherProgram: "Otro programa...",
  onThisComputer: "En este equipo",
  extractWith: "Extraer con",
  extractWithHint:
    "Un programa instalado extrae todos los formatos que abre; el resto va al mejor programa instalado para cada uno.",
  program: "Programa",
  programHint: "El archivo del programa que se ejecuta.",
  arguments: "Argumentos",
  argumentsHint:
    "{archive} es el archivo comprimido y {folder} la carpeta de destino. Ponlos entre comillas, ya que las rutas pueden contener espacios.",
  theProgramPickedIsNo:
    "El programa elegido ya no está, así que los archivos comprimidos van al mejor programa instalado hasta que elijas otro.",
  installedPrograms: "Programas instalados",
  installedProgramsHint: "Buscados en el PATH y donde se instalan 7-Zip, WinRAR y PeaZip.",
  lookAgain: "Volver a buscar",
  noProgramThatExtractsArchives:
    "No se encontró ningún programa que extraiga archivos comprimidos. Instala 7-Zip o indica un programa arriba.",
  onServers: "En servidores",
  archivesOnAnSftpServer:
    "Los archivos comprimidos de un servidor SFTP se extraen en el servidor, con los programas que tenga. Poros busca {programs} la primera vez y usa el que mejor abre cada formato. Los servidores que solo permiten SFTP no pueden extraer; descarga el archivo comprimido para extraerlo aquí.",
  programsFileType: "Programas",

  // lib/archives
  zstandard: "Zstandard",

  // state/archiveActions
  nameTheProgramThatExtracts:
    "Indica el programa que extrae archivos comprimidos en Configuración > Archivos comprimidos.",
  extracting: "Extrayendo {name}",
} satisfies TranslatedArea<"archives">;
