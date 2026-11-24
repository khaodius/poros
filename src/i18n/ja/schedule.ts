import type { TranslatedArea } from "../catalog";

export const schedule = {
  // components/SchedulerDialog
  scheduledTasks: "スケジュールタスク",
  tasks: "タスク",
  newTask: "新しいタスク",
  shownInTheListOf: "タスクの一覧に表示される名前",
  turnedOn: "オン",
  task: "タスク",
  savedConnection: "保存済みの接続",
  ifPorosIsClosedAt: "その時刻に Poros が閉じている場合は、次回の起動時に実行する",
  nextRun: "次回の実行",
  runningNow: "実行中",
  lastRun: "前回の実行",
  chooseTheLocalFolder: "ローカルフォルダーを選択",
  localFolder: "ローカルフォルダー",
  theFolderOnThisComputer: "このコンピューター上のフォルダー",
  serverFolder: "サーバーのフォルダー",
  direction: "方向",
  conflictsAreLeftAloneAnd: "{hint} 競合はそのままにして報告します。",
  rsyncOptions: "rsync のオプション",
  typedAsOnRsyncsCommand:
    "rsync のコマンドラインと同じように入力します。タスクを実行するたびに、上のオプションより優先して適用されます。",
  command: "コマンド",
  inFolderOptional: "実行するフォルダー (省略可)",
  theLoginFolder: "ログイン時のフォルダー",
  when: "実行日時",
  dateAndTime: "日時",
  every: "間隔",
  repeatEvery: "繰り返しの間隔",
  unit: "単位",
  from: "開始",
  starting: "開始日時",
  at: "時刻",
  timeOfDay: "時刻",
  daysOfTheWeek: "曜日",
  onDays: "曜日を指定",
  repeat: "繰り返し",
  once: "1 回のみ",
  synchronizeFolders: "フォルダーの同期",
  transferFiles: "ファイルの転送",
  runACommand: "コマンドの実行",
  uploadTo: "アップロード先",
  downloadFrom: "ダウンロード元",
  copyTo: "コピー先",
  tasksRunWhilePorosIsOpen:
    "タスクは Poros を開いている間に実行されます。cron やタイマーを使って poros-cli から実行することもできます。保存したタスクはここに表示されます。",
  turnedOff: "オフ",
  chooseAConnection: "接続を選択",
  noSavedConnections: "保存済みの接続はありません",
  noSavedSftpConnections: "保存済みの SFTP 接続はありません",
  aDeletedConnection: "削除された接続",
  usesSynchronizingAndRunningCommands:
    "{name} は {label} を使用しています。同期とコマンドの実行には SFTP 接続が必要です。",
  thisTasksSavedConnectionWas:
    "このタスクの保存済みの接続は削除されました。別の接続を選んでください。",
  deleteForGood: "完全に削除",
  starting2: "開始しています...",
  runNow: "今すぐ実行",
  saving: "保存しています...",
  thisConnectionsPasswordIsNotSaved:
    "この接続のパスワードがシステムのキーチェーンに保存されていないため、タスクはログインできません。",
  thePasswordOfIsNot:
    "{names} のパスワードがシステムのキーチェーンに保存されていないため、タスクはログインできません。",
  thePasswordsOfAreNot:
    "{names} のパスワードがシステムのキーチェーンに保存されていないため、タスクはログインできません。",
  saveItInTheConnections: "接続の設定でパスワードを保存してください。",
  saveThemInTheConnections: "各接続の設定でパスワードを保存してください。",
  noneTheTimeHasPassed: "なし (時刻を過ぎています)",

  // components/TransferTaskFields
  aFullPathOnThe: "サーバー上のフルパスです。フォルダーは末尾を / にします",
  aRunNowGoesInto: "今実行すると {folder} に入ります。",
  addFiles: "ファイルを追加",
  addFolders: "フォルダーを追加",
  addTheFilesAndFolders:
    "タスクでアップロードするファイルとフォルダーを追加します。フォルダーは中身ごと転送されます。",
  addTheServerPathsThe:
    "タスクでコピーするサーバーのパス (/var/www/site など) を追加します。フォルダーは中身ごと転送されます。",
  betweenServers: "サーバー間",
  chooseTheFolderToDownload: "ダウンロード先のフォルダーを選択",
  copiesFilesAndFoldersFrom: "このコンピューターからサーバーへファイルとフォルダーをコピーします。",
  copiesFilesAndFoldersFromOne:
    "保存済みの接続から別の保存済みの接続へファイルとフォルダーをコピーします。",
  copiesFilesAndFoldersFromThe:
    "サーバーからこのコンピューターへファイルとフォルダーをコピーします。",
  copyFrom: "コピー元",
  filesAndFoldersOnThe: "サーバー上のファイルとフォルダー",
  filesAndFoldersOnThis: "このコンピューター上のファイルとフォルダー",
  inAFolderOfIts: "専用のフォルダーに入れる (省略可)",
  subfolderExample: "backup-{date}",
  intoTheFolderOnThe: "サーバー上のフォルダーへ",
  intoTheFolderOnThis: "このコンピューター上のフォルダーへ",
  itemsToCopy: "コピーする項目",
  remove: "{path} を削除",
  removeFromTheTask: "タスクから削除",
  serverPathToAdd: "追加するサーバーのパス",
  theFolderToDownloadInto: "ダウンロード先のフォルダー",
  whenAFileIsAlready: "ファイルが既にある場合",
  eachRunCanGetAFolder:
    "実行ごとに、実行時刻から名前を付けたフォルダーを作れます: {placeholders}。",

  // lib/schedule
  overwriteIfTheSourceIs: "転送元が新しい場合は上書き",
  overwriteIfSizeOrDate: "サイズか日時が異なる場合は上書き",
  keepBoth: "両方とも保持",
  everyDay: "毎日",
  weekdays: "平日",
  weekends: "週末",
  onceAt: "1 回のみ、{date}",
  everyMinutes: { other: "{count} 分ごと" },
  everyHours: { other: "{count} 時間ごと" },
  everyDays: { other: "{count} 日ごと" },
  daysAt: "{days} {time}",
  unitMinutes: "分",
  unitHours: "時間",
  unitDays: "日",
  nameRun: "{program} を実行",
  nameUploadOne: "{name} をアップロード",
  nameDownloadOne: "{name} をダウンロード",
  nameCopyOne: "{name} をコピー",
  nameUploadItems: { other: "{count} 個の項目をアップロード" },
  nameDownloadItems: { other: "{count} 個の項目をダウンロード" },
  nameCopyItems: { other: "{count} 個の項目をコピー" },
  nameSync: "{name} を同期",
  patternTooLong: {
    other: "フォルダーのパターンが {count} 文字を超えています",
  },
  patternLoneBrace:
    "フォルダーのパターンに対になっていない波かっこがあります。プレースホルダーは {example} のように書いてください",
  patternUnknown:
    "フォルダーのパターンの {found} はプレースホルダーではありません。{known} を使ってください",
  patternBadName: "フォルダーのパターンが「{name}」になり、フォルダー名に使えません",

  // lib/scheduleDraft
  porosNoLongerKnowsThe:
    "これらの転送のサーバーが Poros に残っていません。代わりにタブからファイルをスケジュールしてください。",
  filesInAnUnlockedVault:
    "ロックを解除した金庫のファイルはスケジュールできません。タスクは金庫のロックを解除できないためです。",
  saveTheConnectionToFirst:
    "先に {label} への接続を保存してください。スケジュールタスクは保存済みの接続でサインインします。",
  selectTheTransfersToSchedule: "スケジュールする転送を選択してください。",
  selectTransfersThatGoThe:
    "スケジュールするには、同じ場所の間を同じ方向に転送するものを選択してください。",
  theSelectedTransfersGoTo:
    "選択した転送は別々のフォルダーに転送されます。1 つのフォルダーに転送するものを選択してください。",
  selectTheFilesToSchedule: "スケジュールするファイルを選択してください。",
  blockedNoServer: "サーバーなし",
  blockedInVault: "金庫内では不可",
  blockedUnsaved: "先に接続を保存してください",

  // state/scheduleActions
  blockedNoTab: "タブなし",
} satisfies TranslatedArea<"schedule">;
