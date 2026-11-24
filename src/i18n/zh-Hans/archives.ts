import type { TranslatedArea } from "../catalog";

export const archives = {
  // components/ArchiveSettings
  theBestOneInstalledFor: "为每种格式选择已安装的最佳程序",
  notFound: "{picked}（未找到）",
  anotherProgram: "其他程序...",
  onThisComputer: "在这台计算机上",
  extractWith: "解压程序",
  extractWithHint: "已安装的程序会解压它能打开的所有格式；其余格式交给为其安装的最佳程序。",
  program: "程序",
  programHint: "要运行的程序文件。",
  arguments: "参数",
  argumentsHint:
    "{archive} 表示压缩文件，{folder} 表示解压到的文件夹。路径中可能有空格，因此请为它们加上引号。",
  theProgramPickedIsNo:
    "所选程序已不存在，因此在您选择其他程序之前，压缩文件会交给已安装的最佳程序。",
  installedPrograms: "已安装的程序",
  installedProgramsHint: "在 PATH 中以及 7-Zip、WinRAR 和 PeaZip 的安装位置中找到。",
  lookAgain: "重新查找",
  noProgramThatExtractsArchives: "未找到可解压压缩文件的程序。请安装 7-Zip，或在上方指定一个程序。",
  onServers: "在服务器上",
  archivesOnAnSftpServer:
    "SFTP 服务器上的压缩文件会在服务器上使用其已有的程序解压。Poros 首次会查找 {programs} 和 {lastProgram}，并为每种格式使用最合适的程序。只允许 SFTP 的服务器无法解压；请将压缩文件下载后在本机解压。",
  programsFileType: "程序",

  // lib/archives
  zstandard: "Zstandard",

  // state/archiveActions
  nameTheProgramThatExtracts: "请在“设置 > 压缩文件”中指定用于解压的程序。",
  extracting: "正在解压 {name}",
} satisfies TranslatedArea<"archives">;
