import type { TranslatedArea } from "../catalog";

export const archives = {
  // components/ArchiveSettings
  theBestOneInstalledFor: "Le meilleur installé pour chaque format",
  notFound: "{picked} (introuvable)",
  anotherProgram: "Autre programme...",
  onThisComputer: "Sur cet ordinateur",
  extractWith: "Extraire avec",
  extractWithHint:
    "Un programme installé extrait tous les formats qu’il sait ouvrir\u202f; les autres vont au meilleur programme installé pour eux.",
  program: "Programme",
  programHint: "Le fichier du programme à lancer.",
  arguments: "Arguments",
  argumentsHint:
    "{archive} est l’archive et {folder} le dossier où elle est extraite. Mettez-les entre guillemets, car les chemins peuvent contenir des espaces.",
  theProgramPickedIsNo:
    "Le programme choisi n’est plus là\u202f; les archives vont donc au meilleur programme installé jusqu’à ce que vous en choisissiez un autre.",
  installedPrograms: "Programmes installés",
  installedProgramsHint: "Trouvés dans le PATH et là où 7-Zip, WinRAR et PeaZip s’installent.",
  lookAgain: "Chercher de nouveau",
  noProgramThatExtractsArchives:
    "Aucun programme d’extraction d’archives n’a été trouvé. Installez 7-Zip, ou indiquez un programme ci-dessus.",
  onServers: "Sur les serveurs",
  archivesOnAnSftpServer:
    "Les archives sur un serveur SFTP sont extraites sur le serveur, avec les programmes qu’il possède. Poros cherche {programs} et {lastProgram} la première fois, et utilise celui qui ouvre le mieux chaque format. Les serveurs qui n’autorisent que SFTP ne peuvent pas extraire\u202f; téléchargez l’archive pour l’extraire ici.",
  programsFileType: "Programmes",

  // lib/archives
  zstandard: "Zstandard",

  // state/archiveActions
  nameTheProgramThatExtracts:
    "Indiquez le programme d’extraction d’archives dans Paramètres > Archives.",
  extracting: "Extraction de {name}",
} satisfies TranslatedArea<"archives">;
