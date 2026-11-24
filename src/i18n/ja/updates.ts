import type { TranslatedArea } from "../catalog";

export const updates = {
  // components/SettingsDialog
  downloadingVersion: "{version} をダウンロードしています",
  downloadingVersionPercent: "{version} をダウンロードしています、{percent}",
  installingVersion: "{version} をインストールしています",
  checkingForUpdates: "アップデートを確認しています",
  versionIsAvailable: "{version} を利用できます",
  upToDateCheckedAt: "最新です ({time} に確認)",
  theVersionYouAreRunning: "実行中のバージョン",
  couldNotListTheReleases: "リリースの一覧を取得できませんでした: {message}",
  noOtherReleaseToInstall: "{version} 以降でインストールできる他のリリースはありません。",
  switchToAnotherRelease: "{version} 以降の別のリリースに切り替えます。",
  versionIsReadyRestart:
    "{version} の準備ができました。切り替えるには Poros を再起動してください。",
  restartingInto: "{version} で再起動しています",
  couldNotInstallVersionBecause: "{version} をインストールできませんでした: {message}",

  // components/StatusBar
  restartToUse: "再起動して {version} を使用",
  restarting: "再起動しています",
  couldNotInstallVersion: "{version} をインストールできませんでした",

  // components/UpdateDialog
  updateAvailable: "アップデートがあります",
  isAvailableYouHave: "Poros {version} を利用できます。現在のバージョンは {current} です。",
  porosKeepsWorkingWhileThe:
    "アップデートのダウンロード中も Poros は使用でき、再起動する前に確認します。",
  skipThisVersion: "このバージョンをスキップ",
  later: "後で",
  update: "アップデート",

  // components/UpdateNotice
  hideTheStatusBarShows: "隠す (ステータスバーから再び表示できます)",
  restartPorosToSwitchTo:
    "今すぐ切り替えるには Poros を再起動してください。再起動しない場合は、次に Poros を開いたときに反映されます。",
  restartNow: "今すぐ再起動",
  of: "{received} / {total}",
  downloadingPoros: "Poros {version} をダウンロードしています",
  installingPoros: "Poros {version} をインストールしています",
  porosIsReady: "Poros {version} の準備ができました",
  restartingPoros: "Poros を再起動しています",
  couldNotInstallPoros: "Poros {version} をインストールできませんでした",

  // state/updateStore
  transfersWillStop: {
    other: "Poros を再起動すると、キューにある {count} 件の転送は停止します。",
  },
  transfersWillStopAndComeBackPaused: {
    other:
      "Poros を再起動すると、キューにある {count} 件の転送は停止し、一時停止した状態で戻ります。",
  },
  unsavedChangesInFilesWill: {
    other: "Poros を再起動すると、{count} 個のファイルの未保存の変更は失われます。",
  },
  theLatestReleaseDoesNotOfferInAppUpdates:
    "GitHub の最新リリースは、まだアプリ内のアップデートに対応していません",
  couldNotCheckForUpdates: "アップデートを確認できませんでした: {message}",
  heldSavesUploadedAfterTheRestart: {
    other:
      "他のプログラムで開いている {count} 個のファイルに、まだアップロードしていない保存内容があります。再起動後に Poros がアップロードを提案します。",
  },
  heldSavesWipedByTheRestart: {
    other:
      "他のプログラムで開いている {count} 個のファイルに、まだアップロードしていない保存内容があります。金庫のファイルは暗号化せずに保持しないため、再起動すると消去されます。",
  },
  heldSavesPartlyWipedByTheRestart: {
    other:
      "他のプログラムで開いている {count} 個のファイルに、まだアップロードしていない保存内容があります。金庫のファイルは消去され、それ以外は再起動後に Poros がアップロードを提案します。",
  },
} satisfies TranslatedArea<"updates">;
