import type { MessageArea } from "../catalog";

export const archives = {
  // components/ArchiveSettings
  theBestOneInstalledFor: "The best one installed for each format",
  notFound: "{picked} (not found)",
  anotherProgram: "Another program...",
  onThisComputer: "On this computer",
  extractWith: "Extract with",
  extractWithHint:
    "An installed program extracts every format it opens; the rest go to the best program installed for them.",
  program: "Program",
  programHint: "The program file to run.",
  arguments: "Arguments",
  argumentsHint:
    "{archive} is the archive and {folder} the folder it goes into. Quote them, as paths can hold spaces.",
  theProgramPickedIsNo:
    "The program picked is no longer there, so archives go to the best one installed until you pick another.",
  installedPrograms: "Installed programs",
  installedProgramsHint: "Found on the PATH and where 7-Zip, WinRAR and PeaZip install themselves.",
  lookAgain: "Look again",
  noProgramThatExtractsArchives:
    "No program that extracts archives was found. Install 7-Zip, or name a program above.",
  onServers: "On servers",
  archivesOnAnSftpServer:
    "Archives on an SFTP server are extracted on the server, with the programs it has. Poros looks for {programs} the first time, and uses the one that opens each format best. Servers that allow SFTP alone cannot extract; download the archive to extract it here.",

  // state/archiveActions
  nameTheProgramThatExtracts: "Name the program that extracts archives in Settings > Archives.",
  // lib/archives
  zstandard: "Zstandard",
  programsFileType: "Programs",
  extracting: "Extracting {name}",
  // components/ArchiveSettings
} as const satisfies MessageArea;
