import type { TranslatedArea } from "../catalog";

export const updates = {
  // components/SettingsDialog
  downloadingVersion: "正在下载 {version}",
  downloadingVersionPercent: "正在下载 {version}，{percent}",
  installingVersion: "正在安装 {version}",
  checkingForUpdates: "正在检查更新",
  versionIsAvailable: "{version} 可用",
  upToDateCheckedAt: "已是最新版本，检查于 {time}",
  theVersionYouAreRunning: "当前运行的版本",
  couldNotListTheReleases: "无法列出版本：{message}",
  noOtherReleaseToInstall: "{version} 及以后没有其他可安装的版本。",
  switchToAnotherRelease: "切换到 {version} 及以后的其他版本。",
  versionIsReadyRestart: "{version} 已就绪。重启 Poros 即可切换到该版本。",
  restartingInto: "正在重启到 {version}",
  couldNotInstallVersionBecause: "无法安装 {version}：{message}",

  // components/StatusBar
  restartToUse: "重启以使用 {version}",
  restarting: "正在重启",
  couldNotInstallVersion: "无法安装 {version}",

  // components/UpdateDialog
  updateAvailable: "有可用更新",
  isAvailableYouHave: "Poros {version} 已推出。您当前的版本是 {current}。",
  porosKeepsWorkingWhileThe: "下载更新期间 Poros 可继续使用，重启前会先询问您。",
  skipThisVersion: "跳过此版本",
  later: "稍后",
  update: "更新",

  // components/UpdateNotice
  hideTheStatusBarShows: "隐藏；状态栏会再次显示",
  restartPorosToSwitchTo: "立即重启 Poros 以切换到新版本，或在下次打开 Poros 时生效。",
  restartNow: "立即重启",
  of: "{received} / {total}",
  downloadingPoros: "正在下载 Poros {version}",
  installingPoros: "正在安装 Poros {version}",
  porosIsReady: "Poros {version} 已就绪",
  restartingPoros: "正在重启 Poros",
  couldNotInstallPoros: "无法安装 Poros {version}",

  // state/updateStore
  transfersWillStop: {
    other: "Poros 重启时，队列中的 {count} 项传输将会停止。",
  },
  transfersWillStopAndComeBackPaused: {
    other: "Poros 重启时，队列中的 {count} 项传输将会停止，并在重启后以暂停状态恢复。",
  },
  unsavedChangesInFilesWill: {
    other: "Poros 重启时，{count} 个文件中未保存的更改将会丢失。",
  },
  theLatestReleaseDoesNotOfferInAppUpdates: "GitHub 上的最新版本尚不支持应用内更新",
  couldNotCheckForUpdates: "无法检查更新：{message}",
  heldSavesUploadedAfterTheRestart: {
    other: "在其他程序中打开的 {count} 个文件有尚未上传的保存内容。重启后 Poros 会提示上传它们。",
  },
  heldSavesWipedByTheRestart: {
    other:
      "在其他程序中打开的 {count} 个文件有尚未上传的保存内容。由于保险库文件不会以未加密形式保留，重启会清除这些内容。",
  },
  heldSavesPartlyWipedByTheRestart: {
    other:
      "在其他程序中打开的 {count} 个文件有尚未上传的保存内容。来自保险库的内容会被清除；其他内容重启后 Poros 会提示上传。",
  },
} satisfies TranslatedArea<"updates">;
