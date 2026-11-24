import type { TranslatedArea } from "../catalog";

export const archives = {
  // components/ArchiveSettings
  theBestOneInstalledFor: "形式ごとに最適なインストール済みのプログラム",
  notFound: "{picked} (見つかりません)",
  anotherProgram: "別のプログラム...",
  onThisComputer: "このコンピューター",
  extractWith: "展開に使うプログラム",
  extractWithHint:
    "インストール済みのプログラムは、開けるすべての形式を展開します。それ以外の形式は、その形式に最適なインストール済みのプログラムで展開します。",
  program: "プログラム",
  programHint: "実行するプログラムのファイルです。",
  arguments: "引数",
  argumentsHint:
    "{archive} はアーカイブ、{folder} は展開先のフォルダーです。パスには空白が含まれることがあるため、引用符で囲んでください。",
  theProgramPickedIsNo:
    "選んだプログラムが見つからないため、別のプログラムを選ぶまでは、インストール済みの最適なプログラムでアーカイブを展開します。",
  installedPrograms: "インストール済みのプログラム",
  installedProgramsHint: "PATH と、7-Zip、WinRAR、PeaZip のインストール先から見つけたものです。",
  lookAgain: "もう一度探す",
  noProgramThatExtractsArchives:
    "アーカイブを展開できるプログラムが見つかりませんでした。7-Zip をインストールするか、上でプログラムを指定してください。",
  onServers: "サーバー",
  archivesOnAnSftpServer:
    "SFTP サーバー上のアーカイブは、サーバーにあるプログラムを使ってサーバー上で展開します。Poros は初回に {programs}、{lastProgram} を探し、形式ごとに最も適したものを使います。SFTP しか許可していないサーバーでは展開できません。アーカイブをダウンロードしてこのコンピューターで展開してください。",
  programsFileType: "プログラム",

  // lib/archives
  zstandard: "Zstandard",

  // state/archiveActions
  nameTheProgramThatExtracts:
    "「設定 > アーカイブ」で、アーカイブを展開するプログラムを指定してください。",
  extracting: "{name} を展開しています",
} satisfies TranslatedArea<"archives">;
