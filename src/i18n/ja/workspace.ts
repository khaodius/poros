import type { TranslatedArea } from "../catalog";

export const workspace = {
  // App
  connectToServer: "サーバーに接続",
  synchronizeFolders: "フォルダーの同期",
  scheduledTasks: "スケジュールタスク",
  settingsCtrl: "設定 (Ctrl+,)",

  // components/BottomPanel
  queue: "キュー",
  failed: "失敗",
  completed: "完了",
  startQueue: "キューを開始",
  pauseQueue: "キューを一時停止",
  pauseSelected: "選択項目を一時停止",
  resumeSelected: "選択項目を再開",
  moveSelectedToTheTop: "選択項目を先頭へ移動",
  moveSelectedToTheBottom: "選択項目を末尾へ移動",
  removeSelected: "選択項目を削除",
  retryAll: "すべて再試行",
  retrySelected: "選択項目を再試行",
  clearFailedTransfers: "失敗した転送をクリア",
  keepEachTransferInView: "転送が終わるたびに、その転送を表示し続けます",
  follow: "追従",
  clearFinishedTransfers: "完了した転送をクリア",
  simultaneousTransfers: "同時に実行する転送の数",
  workers: "同時転送",
  log: "ログ",

  // components/CloseTabDialog
  close: "{label} を閉じますか?",
  transfersIntoOrOutOf:
    "ロックを解除した金庫との間の転送は、どちらを選んでも一時停止します。タブを閉じると金庫がロックされるためです。金庫のロックをもう一度解除してから再開してください。",
  pauseTransfersAndClose: "転送を一時停止して閉じる",
  keepTransferring: "転送を続ける",
  transfersForThisServerStillQueued: {
    other:
      "このサーバーの転送が {count} 件、まだキューにあります。タブを閉じた後もそれぞれの接続で実行を続けるか、再開するまで一時停止したまま待つことができます。",
  },
  unmountAndClose: "マウントを解除して閉じる",

  // components/DockArea
  dragToResizeDoubleClick: "ドラッグでサイズを変更、ダブルクリックで均等にします",
  moveToMainWindow: "メインウィンドウへ移動",
  newWindow: "新しいウィンドウ",
  moveToThatWindow: "そのウィンドウへ移動",

  // components/DragGhost
  uploadTo: "{folder} にアップロード",
  downloadTo: "{folder} にダウンロード",
  copyTo: "{folder} にコピー",
  copyTo2: "{baseName} にコピー",
  moveTo: "{baseName} に移動",

  // components/DropMarker
  splitRight: "右に分割",
  splitDown: "下に分割",
  mergeAsTab: "タブとして結合",
  splitLeft: "左に分割",
  splitUp: "上に分割",

  // components/RestoringPane
  signIn: "サインイン",
  closeTab: "タブを閉じる",
  reopensNameOnceConnected: "接続すると {name} をもう一度開きます。",
  opensANewShellOnceConnected: "接続すると新しいシェルを開きます。",
  reopensOnceConnected: "接続するともう一度開きます。",
  openingName: "{name} を開いています...",
  openingFile: "ファイルを開いています...",
  reconnectingTo: "{label} に再接続しています...",
  signInTo: "{label} にサインイン",
  couldNotReconnectTo: "{label} に再接続できませんでした",
  porosKeepsNoPasswordForThisServer: "Poros はこのサーバーのパスワードを保存していません。",

  // components/SplashScreen
  loadingPoros: "Poros を読み込んでいます",

  // components/StatusBar
  connectionToLost: "{label} への接続が切れました",
  connectedTo: "{label} に接続中",
  serversConnected: { other: "{count} 台のサーバーに接続中" },
  notConnected: "未接続",
  updateTo: "{version} にアップデート",
  uploadSpeed: "アップロード速度",
  downloadSpeed: "ダウンロード速度",
  appVersion: "Poros {version}",
  transfersLeft: { other: "残り {count} 件" },
  aboutDuration: "約 {duration}",
  queueStatusPaused: "{status} (一時停止中)",

  // components/TabBar
  openTerminal: "ターミナルを開く",
  moveToNewWindow: "新しいウィンドウへ移動",
  closeOtherTabs: "他のタブを閉じる",
  newTabCtrlT: "新しいタブ (Ctrl+T)",
  newTab: "新しいタブ",
  disconnected: "切断されました",
  unsavedChanges: "未保存の変更",
  close2: "{label} を閉じる",
  closeCtrlW: "閉じる (Ctrl+W)",
  mountAsDrive: "ドライブとしてマウント...",
  mountedAsADriveAt: "{paths} にドライブとしてマウント中",
  mountedAsADrive: "ドライブとしてマウント中",

  // components/TabContent
  local: "ローカル",
  thisSessionHasEnded: "このセッションは終了しました",
  connectionLost: "接続が切れました",
  reconnectingInS: "{secondsLeft} 秒後に再接続します",
  reconnecting: "再接続しています...",
  reconnectingInSecondsAttempt: "{seconds} 秒後に再接続します ({attempt} 回目)",
  reconnectingAttempt: "再接続しています... ({attempt} 回目)",
  reconnectNow: "今すぐ再接続",
  reconnect: "再接続",

  // components/WelcomePanel
  localFiles: "ローカルファイル",
  import: "インポート",
  importFromFilezillaWinscpAnd: "FileZilla、WinSCP などのアプリからインポート",
  savedConnections: "保存済みの接続",
  serversYouSaveFromThe:
    "接続ダイアログやサーバータブのブックマークボタンから保存したサーバーがここに表示されます。FileZilla、WinSCP、PuTTY などのアプリに保存したサイトもインポートできます。",
  connecting: "接続しています...",

  // components/WindowControls
  minimize: "最小化",
  restore: "元に戻す",
  maximize: "最大化",

  // components/Workspace
  dragToResizeDoubleClickTo: "ドラッグでサイズを変更、ダブルクリックで元に戻します",

  // hooks/useTabLabel
  closedSession: "終了したセッション",

  // state/restoreActions
  couldNotReopen: "{name} をもう一度開けませんでした: {message}",
  porosNoLongerKnowsHow: "このサーバーへの接続方法が Poros に残っていません。",
} satisfies TranslatedArea<"workspace">;
