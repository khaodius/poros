import type { TranslatedArea } from "../catalog";

export const archives = {
  // components/ArchiveSettings
  theBestOneInstalledFor: "Das beste installierte Programm je Format",
  notFound: "{picked} (nicht gefunden)",
  anotherProgram: "Anderes Programm...",
  onThisComputer: "Auf diesem Computer",
  extractWith: "Entpacken mit",
  extractWithHint:
    "Ein installiertes Programm entpackt jedes Format, das es öffnet. Die übrigen gehen an das beste dafür installierte Programm.",
  program: "Programm",
  programHint: "Die Programmdatei, die ausgeführt wird.",
  arguments: "Argumente",
  argumentsHint:
    "{archive} ist das Archiv und {folder} der Ordner, in den es entpackt wird. Setzen Sie beide in Anführungszeichen, da Pfade Leerzeichen enthalten können.",
  theProgramPickedIsNo:
    "Das gewählte Programm ist nicht mehr vorhanden. Archive gehen deshalb an das beste installierte, bis Sie ein anderes wählen.",
  installedPrograms: "Installierte Programme",
  installedProgramsHint:
    "Gefunden im PATH und dort, wo sich 7-Zip, WinRAR und PeaZip installieren.",
  lookAgain: "Erneut suchen",
  noProgramThatExtractsArchives:
    "Es wurde kein Programm zum Entpacken von Archiven gefunden. Installieren Sie 7-Zip oder geben Sie oben ein Programm an.",
  onServers: "Auf Servern",
  archivesOnAnSftpServer:
    "Archive auf einem SFTP-Server werden auf dem Server entpackt, mit den Programmen, die er hat. Poros sucht beim ersten Mal nach {programs} und verwendet für jedes Format das Programm, das es am besten öffnet. Server, die nur SFTP erlauben, können nicht entpacken. Laden Sie das Archiv dann herunter, um es hier zu entpacken.",
  programsFileType: "Programme",

  // lib/archives
  zstandard: "Zstandard",

  // state/archiveActions
  nameTheProgramThatExtracts:
    "Geben Sie unter Einstellungen > Archive das Programm zum Entpacken an.",
  extracting: "{name} wird entpackt",
} satisfies TranslatedArea<"archives">;
