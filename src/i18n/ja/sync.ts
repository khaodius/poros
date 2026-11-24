import type { TranslatedArea } from "../catalog";

export const sync = {
  // components/SyncDialog
  chooseTheLocalFolder: "ローカルフォルダーを選択",
  synchronizeFolders: "フォルダーの同期",
  localFolder: "ローカルフォルダー",
  theFolderOnThisComputer: "このコンピューター上のフォルダー",
  server: "サーバー",
  noSftpServerConnected: "SFTP サーバーに接続していません",
  serverFolder: "サーバーのフォルダー",
  direction: "方向",
  rsyncOptions: "rsync のオプション",
  schedule: "スケジュール...",
  runThisSynchronizationAtSet: "この同期を決まった時刻に実行します",
  fillInBothFoldersOf:
    "スケジュールするには、保存済みの接続を選び、両方のフォルダーを入力してください",
  comparingContentsOfFiles: {
    other: "内容を比較中: {count} 個中 {compared} 個のファイル",
  },
  startingFromTheDefaultsIn:
    "設定の既定値に、{siteName} 用に保存した rsync 引数を適用した状態から始まります。ここでの変更はこの同期にだけ適用されます。",
  startingFromTheDefaultsInSettings:
    "設定の既定値から始まります。ここでの変更はこの同期にだけ適用されます。",
  compareDryRun: "比較 (テスト実行)",
  compare: "比較",
  listingFoldersLocalAndServer:
    "フォルダーの一覧を取得中: ローカル {local} 項目、サーバー {remote} 項目",

  // components/SyncPlanList
  rsync: "rsync {arguments}",
  theOptionsAsRsyncArguments: "rsync 引数で表したオプション",
  dryRunRunningThisChanges:
    "テスト実行: 実行しても何も変更せず、行われる内容をログに書き込みます。",
  unchangedSourceFilesWillBe: {
    other: "変更のない転送元のファイル {count} 個も削除されます。",
  },
  includeAll: "すべて含める",
  action: "操作",
  why: "理由",
  entriesPassedOverBrokenOr: {
    other:
      "{count} 個の項目を飛ばしました: リンク切れやループするリンク、特殊ファイル、読み取れなかったフォルダー",
  },
  include: "{path} を含める",
  settle: "{path} を解決",
  leaveAsIs: "そのままにする",
  uploadLocalCopy: "ローカルのコピーをアップロード",
  downloadServerCopy: "サーバーのコピーをダウンロード",
  files: { other: "{count} 個のファイル" },
  all: "すべて",
  conflicts: "競合",
  noFilesToCopy: "コピーするファイルはありません。",
  everythingIsInSync: "すべて同期済みです。",
  starting: "開始しています...",
  settleByHand: "手動で解決",
  dryRun: "テスト実行",
  synchronize: "同期",

  // components/SyncSettings
  rsync2: "rsync",
  rsyncRunsOnTheServer:
    "rsync は SSH 接続を通してサーバー上で実行されるため、サーバーに rsync がインストールされている必要があります。rsync デーモン (ポート 873 の rsync:// アドレス) には対応していません。",
  useRsync: "rsync を使う",
  useRsyncHint:
    "相手側に既にあるファイルは、変更された部分だけを送って更新します。オフの場合やサーバーに rsync がない場合は、SFTP でファイル全体をコピーします。",
  forFilesLargerThan: "rsync を使うファイルの最小サイズ",
  forFilesLargerThanHint: "小さなファイルは全体をコピーする方が速く済みます。",
  rsyncOnTheServer: "サーバーの rsync",
  rsyncOnTheServerHint: "rsync を起動するコマンドです。PATH にないサーバーで指定します。",
  folderSynchronization: "フォルダーの同期",
  eachSynchronizationStartsWithThese:
    "各同期はこれらのオプションから始まります。保存したサイト独自の rsync 引数がその上に適用され、「フォルダーの同期」で 1 回分だけ変更することもできます。",
  typedAsOnRsyncsCommand:
    "rsync のコマンドラインと同じように入力します。すべての同期で、上のオプションより優先して適用されます。",

  // lib/sync
  folders: { other: "{count} 個のフォルダー" },
  files2: { other: "{count} 個のファイル ({size})" },
  deleteItems: { other: "{count} 個の項目を削除" },
  deletionsHeldBackByThe: {
    other: "上限のため保留した削除 {count} 件",
  },
  emptyFoldersLeftOut: {
    other: "除外した空のフォルダー {count} 個",
  },
  wouldCopyFiles: {
    other: "コピー予定: {count} 個のファイル ({size})",
  },
  deleteItems2: { other: "削除予定: {count} 個の項目" },
  createFolders: { other: "作成予定: {count} 個のフォルダー" },
  queuedFiles: { other: "キューに追加: {count} 個のファイル ({size})" },
  deletedItems: { other: "削除: {count} 個の項目" },
  itemsToDeleteOnceCopying: {
    other: "コピー後に削除: {count} 個の項目",
  },
  createdFolders: { other: "作成: {count} 個のフォルダー" },
  createdLinks: { other: "作成: {count} 個のリンク" },
  updatedFiles: { other: "更新: {count} 個のファイル" },
  removedUnchangedSourceFiles: {
    other: "変更のない転送元を削除: {count} 個のファイル",
  },
  localToServer: "ローカルからサーバー",
  makesTheServerFolderMatch: "サーバーのフォルダーをローカルのフォルダーと同じにします。",
  serverToLocal: "サーバーからローカル",
  makesTheLocalFolderMatch: "ローカルのフォルダーをサーバーのフォルダーと同じにします。",
  bothWays: "双方向",
  copiesNewFilesEachWay:
    "新しいファイルを双方向にコピーします。両側にファイルがある場合は、新しい方が優先されます。",
  deleteLocal: "ローカルで削除",
  deleteOnServer: "サーバーで削除",
  conflict: "競合",
  plannedUpload: "{total}をアップロード",
  plannedDownload: "{total}をダウンロード",
  conflictsLeft: {
    other: "{count} 件の競合はそのまま",
  },
  plannedSentences: "{parts}。",
  plannedSeparator: "。",
  nothingSelected: "何も選択されていません。",
  onlyInTheLocalFolder: "ローカルのフォルダーのみ",
  onlyOnTheServer: "サーバーのみ",
  sizeOrTimeDiffers: "サイズか日時が異なる",
  contentsDiffer: "内容が異なる",
  copiedWithoutComparing: "比較せずにコピー",
  newerInTheLocalFolder: "ローカルのフォルダーの方が新しい",
  newerOnTheServer: "サーバーの方が新しい",
  notInTheLocalFolder: "ローカルのフォルダーにない",
  notOnTheServer: "サーバーにない",
  typeDiffers: "こちらは{here}、サーバーは{there}",
  differsWithTheSameTimeOnBothSides: "異なりますが、両側で日時は同じです",
  shorterOnTheTarget: "転送先の方が短いため、残りだけを送信します",
  sameContentsOnlyTheTimeOrPermissions: "内容は同じため、日時かパーミッションだけを設定します",
  nothingHere: "なし",
  folderHere: "フォルダー",
  linkHere: "リンク",
  fileHere: "ファイル",
  nothingThere: "なし",
  folderThere: "フォルダー",
  linkThere: "リンク",
  fileThere: "ファイル",
  countUnchanged: { other: "変更なし {count} 個" },
  countKeptAsTheyAre: { other: "そのまま {count} 個" },
  countOnlyInTheLocalFolderNotDeleted: {
    other: "ローカルのフォルダーのみ {count} 個 (削除しません)",
  },
  countOnlyOnTheServerNotDeleted: {
    other: "サーバーのみ {count} 個 (削除しません)",
  },
  countProtectedFromDeletion: {
    other: "削除から保護 {count} 個",
  },
  countNewNotCreated: { other: "新規 {count} 個 (作成しません)" },
  countOutsideTheSizeLimits: {
    other: "サイズの範囲外 {count} 個",
  },
  countExcluded: { other: "除外 {count} 個" },
  countPassedOver: { other: "スキップ {count} 個" },
  dryRunSummary: "テスト実行 ({parts})。何も変更していません。詳しくはログを参照してください。",
  runSummary: "{done}。",
  runSummaryWithFailures: {
    other: "{done}。{count} 件の変更が失敗しました。詳しくはログを参照してください。",
  },
} satisfies TranslatedArea<"sync">;
