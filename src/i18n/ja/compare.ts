import type { TranslatedArea } from "../catalog";

export const compare = {
  // components/CompareDialog
  compareFiles: "ファイルの比較",
  swapSides: "左右を入れ替え",
  onlyChanges: "変更箇所のみ",
  ignoreWhitespace: "空白を無視",
  ignoreCase: "大文字と小文字を区別しない",
  onTheLeftOnThe: "左が {left}、右が {right} です。",
  readingBothFiles: "両方のファイルを読み込んでいます...",
  previousChangeAltUp: "前の変更箇所 (Alt+Up)",
  previousChange: "前の変更箇所",
  nextChangeAltDown: "次の変更箇所 (Alt+Down)",
  nextChange: "次の変更箇所",
  changes: { other: "{count} 個の変更箇所" },
  showUnchangedLines: { other: "変更のない {count} 行を表示" },
  comparing: "比較しています...",
  noChanges: "変更はありません",
  changeOf: "変更箇所 {current}/{count}",

  // components/FolderCompareBar
  synchronizeTheseFolders: "これらのフォルダーを同期",
  synchronizeTheseFolders2: "これらのフォルダーを同期 ({unavailable})",
  folderComparison: "フォルダーの比較",
  comparedWith: "比較対象: {where}",
  comparedWithAt: "比較対象: {where}: {path}",
  again: "もう一度{action}",
  selectByState: "状態で選択",
  comparisonOptions: "比較のオプション",
  stopComparing: "比較を終了 ({shortcut})",
  rowsByHowTheyCompare: "比較結果ごとの行数",
  comparingSubfoldersItemsListedSo: {
    other: "サブフォルダーを比較中: これまでに {count} 個の項目を取得",
  },
  couldNotCompareSubfolders: "サブフォルダーを比較できませんでした: {message}",
  thereAreTooManySubfolders:
    "サブフォルダーが多すぎるため、すべてには印を付けられません。最も深い階層には印が付きません。",
  thePanesAreNoLonger: "{note}。ペインは連動していません。",
  inAnotherWindow: "別のウィンドウの {label}",
  nothingToCompareYet: "比較するものがまだありません",
  stateCount: "{state}: {amount}",
  stateCountHidden: "{state}: {amount} (非表示)",

  // components/folderCompareMenu
  selectNewerAndOnlyHere: "新しい項目とこちらのみの項目を選択",
  selectDifferent: "異なる項目を選択",
  hideIdenticalItems: "同一の項目を非表示",
  compareBySizeOnly: "サイズだけで比較",
  ignoreTimeZoneOffsetsWhole: "タイムゾーンのずれ (1 時間単位) を無視",
  includeHiddenFiles: "隠しファイルを含める",
  browseBothPanesTogether: "両方のペインを連動して移動",
  compareFolders: "フォルダーを比較",
  compareContentsOfSubfolders: "サブフォルダーの内容を比較",

  // hooks/useFolderCompare
  syncOnlyLocalAndSftp: "このコンピューターと SFTP サーバーの間のみ",
  syncFromServerWindow: "サーバーのタブがあるウィンドウからのみ",

  // lib/compare
  lfUnix: "LF (Unix)",
  crlfWindows: "CRLF (Windows)",
  lineEndingNone: "なし",
  lineEndingMixed: "混在",
  lineEndingsDiffer: "改行コードが異なります。左は {left}、右は {right} です。",
  filesIdentical: "ファイルは同一です。",
  filesDifferTooLarge: "ファイルは異なります。大きすぎるため並べて表示できません。",
  filesDifferNotText: "ファイルは異なります。テキストではないため、バイト単位でのみ比較しました。",
  linesChanged: { other: "変更された行 {count} 行" },
  linesAdded: { other: "追加された行 {count} 行" },
  linesRemoved: { other: "削除された行 {count} 行" },
  lineCounts: "{counts}。",
  onlyLineEndingsDiffer: "改行コードだけが異なります。",
  filesDifferInIgnored: "ファイルの違いは、無視している部分だけです。",
  filesDifferInBytes: "行は同じですが、ファイルのバイトが異なります。",

  // lib/folderCompare
  differsInside: "中身が異なります: {found}",
  noDifferencesFoundInsideBut: "中身に違いは見つかりませんでしたが、一部は未比較です: {found}",
  identicalInsideFiles: {
    other: "中身は同一です: {count} 個のファイル",
  },
  identicalInsideNoFilesOn: "中身は同一です: どちらにもファイルはありません",
  newerHere: "こちらが新しい",
  olderHere: "こちらが古い",
  onlyHere: "こちらのみ",
  different: "異なる",
  identical: "同一",
  differentAndNeitherIsNewer: "異なりますが、どちらも新しくはありません",
  onBothSidesTheContents: "両側にあります。内容は比較していません。",
  neitherListingRefreshed:
    "どちらのペインの一覧も更新できなかったため、両方の一覧を再取得するまで印が古い可能性があります",
  thisListingNotRefreshed:
    "このペインの一覧を更新できなかったため、一覧を再取得するまで印が古い可能性があります",
  partnerListingNotRefreshed:
    "{partner} の一覧を更新できなかったため、一覧を再取得するまで印が古い可能性があります",
  pausedBothShowResults:
    "両方のペインに検索結果が表示されている間は、比較を一時停止します。結果を閉じると印が戻ります",
  pausedThisShowsResults:
    "このペインに検索結果が表示されている間は、比較を一時停止します。結果を閉じると印が戻ります",
  pausedPartnerShowsResults:
    "{partner} に検索結果が表示されている間は、比較を一時停止します。結果を閉じると印が戻ります",
  summaryNewerHere: { other: "こちらが新しい {count} 個" },
  summaryOlderHere: { other: "こちらが古い {count} 個" },
  summaryOnlyHere: { other: "こちらのみ {count} 個" },
  summaryOnlyThere: { other: "もう一方のペインのみ {count} 個" },
  summaryDifferent: { other: "異なる {count} 個" },
  summaryIdentical: { other: "同一 {count} 個" },
  summaryNotCompared: { other: "未比較 {count} 個" },

  // state/folderCompare
  noFolderToCompare: "このペインには比較するフォルダーがありません",
  openASecondPane: "比較するには、フォルダーのペインをもう 1 つ開いてください",
  paneShowsResults: "{pane} に検索結果が表示されています",
  noFolderNamed: "{pane} に {name} という名前のフォルダーはありません",
  noFolderAbove: "{pane} にはこのフォルダーより上のフォルダーがありません",
  paneCouldNotOpen: "{pane} で {path} を開けませんでした",
} satisfies TranslatedArea<"compare">;
