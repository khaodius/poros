import type { TranslatedArea } from "../catalog";

export const transfers = {
  // components/TransferLookSettings
  timeLeft: "残り {duration}",

  // components/TransferView
  copySourcePaths: "転送元のパスをコピー",
  copySourcePath: "転送元のパスをコピー",
  moveToTop: "先頭へ移動",
  moveToBottom: "末尾へ移動",
  progress: "進行状況",
  speed: "速度",
  status: "状態",
  dragFilesBetweenTabsOr:
    "タブ間でファイルをドラッグするか、ファイルのメニューの「アップロード」または「ダウンロード」を使用してください。",
  transfersThatFailAfterTheir: "再試行しても失敗した転送がここに表示されます。",
  finishedTransfersAreListedHere: "完了した転送がここに表示されます。",
  done: "完了",
  scheduleThese: "スケジュールに登録...",
  scheduleThis: "スケジュールに登録...",
  connectionLostWaitingForTheServer: "接続が切れました。サーバーを待っています",
  waitingToRetry: "再試行待ち ({attempt} 回目)",
  queued: "待機中",
  listingFolder: "フォルダーの内容を取得中",
  updatingWithRsyncSent: "rsync で更新中、{size} 送信済み",
  copyingDirectlyBetweenTheServers: "サーバー間で直接コピー中",
  transferringOnConnections: {
    other: "{count} 本の接続で転送中",
  },
  transferring: "転送中",
  paused: "一時停止中",
  alreadyExistsWaitingForYourChoice: "既に存在します。選択を待っています",
  doneCopiedDirectlyBetweenTheServers: "完了 (サーバー間で直接コピー)",
  doneWithRsyncSent: "完了 (rsync、{size} 中 {sent} を送信)",
  skippedBecause: "スキップ: {error}",
  skipped: "スキップ",
  failedBecause: "失敗: {error}",
  failedUnknownError: "失敗: 不明なエラー",
  rowTitle: "転送元: {source}\n転送先: {target}",
  rowTitleQueuedByTask: "転送元: {source}\n転送先: {target}\nタスク「{task}」が追加",

  // lib/progressBars
  sizeOfSize: "{transferred} / {size}",

  // state/transferActions
  nothingToTransferOnlyRegular:
    "転送するものがありません。コピーできるのは通常のファイルとフォルダーだけです。",
  unlockTheVaultToCopy: "ファイルをコピーするには、金庫のロックを解除してください。",
  couldNotCheckWhetherThis: "このフォルダーが金庫かどうかを確認できませんでした。",
  isALockedVaultOpen:
    "{baseName} はロックされた金庫です。ファイルをコピーするには、金庫を開いてください。",
  dropLocalFilesOnA:
    "アップロードするには、ローカルのファイルをサーバーのタブにドロップしてください。",
  copyingTo: "{folder} にコピー中",
  movedTo: "{folder} に移動しました",
  copiedTo: "{folder} にコピーしました",
  connectToAServerTo: "アップロードするには、サーバーに接続してください。",
  openALocalTabTo: "ダウンロード先のローカルタブを開いてください。",
  dropFilesOnAServer: "アップロードするには、ファイルをサーバーのタブにドロップしてください。",
  local: "ローカル",
} satisfies TranslatedArea<"transfers">;
