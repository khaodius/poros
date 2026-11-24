import type { TranslatedArea } from "../catalog";

export const automation = {
  // components/AutomationSettings
  whenTheTransferQueueFinishes: "転送キューが終わったとき",
  then: "動作",
  thenHint:
    "キューのツールバーからも選べます。コマンド以外は、止められるように先に 30 秒のカウントダウンを行います。",
  onlyTheNextTime: "次の 1 回だけ",
  onlyTheNextTimeHint: "1 回実行すると「何もしない」に戻ります。",
  command: "コマンド",
  commandHint:
    "「コマンドを実行」で使います。このコンピューターで実行します。{done}、{failed}、{skipped}、{bytes} には合計が入ります。",
  aCommandOrScript: "コマンドまたはスクリプト",
  showANotification: "通知を表示する",
  onlyWhenPorosIsIn: "Poros がバックグラウンドにあるときだけ",
  playASound: "音を鳴らす",
  serverCommands: "サーバーのコマンド",
  listedUnderCommandsWhenYou:
    "サーバーのペインを右クリックしたときの「コマンド」に表示されます。表示中のフォルダーでサーバーのシェルを通して実行され、次の値が入ります:",
  scheduledTasks: "スケジュールタスク",
  synchronizeFoldersOrRunCommands: "決まった時刻にフォルダーを同期したりコマンドを実行したりします",
  synchronizeFoldersOrRunCommandsHint:
    "タスクは Poros を開いている間、保存済みの接続を使ってバックグラウンドで実行されます。",
  output: "出力",
  openAWindowWithWhat:
    "コマンドが出力した内容をウィンドウに表示します。オフにすると何も表示せずに実行します",
  reloadTheServersFoldersWhen: "コマンドが終わったらサーバーのフォルダーを再読み込みする",
  deleteCommand: "コマンドを削除",
  nameOfTheNewCommand: "新しいコマンドの名前",
  newCommand: "新しいコマンド",

  // components/CommandOutputDialog
  earlierOutputWasDropped: "以前の出力は破棄されました。",
  theCommandPrintedNothing: "コマンドは何も出力しませんでした。",
  running: "実行中",
  commandIn: "{folder} で実行: {command}",
  commandsIn: { other: "{folder} で実行: {count} 個のコマンド" },
  theLoginFolder: "ログイン時のフォルダー",

  // components/RunCommandDialog
  runACommand: "コマンドを実行",
  runACommandOnThe: "サーバーでコマンドを実行",
  commandToRunIn: "{folder} で実行するコマンド",
  refreshTheServersFoldersWhen: "終わったらサーバーのフォルダーを更新する",
  selectFilesOrFoldersTo:
    "プレースホルダーに値を入れるには、ファイルまたはフォルダーを選択してください。",
  run: "実行",

  // components/WhenDoneControl
  whatHappensWhenTheQueue: "キューが終わったときの動作",
  whenDone: "完了時",
  whenTheQueueFinishes: "キューが終わったとき",

  // components/WhenDoneCountdown
  theTransferQueueHasFinished: "転送キューが終わりました。{seconds}後に{title}を実行します。",
  seconds: { other: "{count} 秒" },

  // components/commandMenu
  commands: "コマンド",
  runACommand2: "コマンドを実行...",
  editCommands: "コマンドを編集...",

  // lib/automation
  filesTransferredIn: {
    other: "{count} 個のファイルを転送しました ({size}、{duration})",
  },
  doNothing: "何もしない",
  closePoros: "Poros を終了",
  lockTheComputer: "コンピューターをロック",
  sleep: "スリープ",
  hibernate: "休止状態",
  logOff: "サインアウト",
  shutDown: "シャットダウン",
  closingPoros: "Poros の終了",
  lockingTheComputer: "コンピューターのロック",
  puttingTheComputerToSleep: "スリープへの移行",
  hibernatingTheComputer: "休止状態への移行",
  loggingOff: "サインアウト",
  shuttingDownTheComputer: "シャットダウン",
  confirmCloseNow: "今すぐ終了",
  confirmLockNow: "今すぐロック",
  confirmSleepNow: "今すぐスリープ",
  confirmHibernateNow: "今すぐ休止状態にする",
  confirmLogOffNow: "今すぐサインアウト",
  confirmShutDownNow: "今すぐシャットダウン",
  transfersFinishedWithErrors: "転送が終わりました (エラーあり)",
  transfersFinished: "転送が終わりました",
  failedCount: "{count} 個が失敗",
  skippedCount: "{count} 個をスキップ",

  // lib/commands
  eachSelectedItemsFullPath: "選択した各項目のフルパス (項目ごとに 1 回実行)",
  eachSelectedItemsNameRunning: "選択した各項目の名前 (項目ごとに 1 回実行)",
  theFullPathsOfAll: "選択したすべての項目のフルパス",
  theNamesOfAllSelected: "選択したすべての項目の名前",
  theFolderShown: "表示中のフォルダー",
  stoppedAfter: "{elapsed} 後に停止しました",
  endedBySignal: "{elapsed} 後にシグナル {signal} で終了しました",
  endedWithoutStatus: "{elapsed} 後に終了コードなしで終了しました",
  exitedWithStatus: "{elapsed} 後に終了コード {status} で終了しました",
  finishedIn: "{elapsed} で完了しました",

  // state/commandStore
  stopped: "停止しました",
  nothingToRun: "実行するものがありません",
  allCommandsFinished: "{finished} 個のコマンドがすべて完了しました",
  ofCommandsFailed: "{finished} 個のコマンドのうち {failed} 個が失敗しました",

  // state/whenDone
  whenTheQueueFinished: "キューが終わったときの動作",
  theCommandAfterTheQueue: "キューが終わった後のコマンド",
} satisfies TranslatedArea<"automation">;
