import type { TranslatedArea } from "../catalog";

export const dialogs = {
  // components/ConflictDialog
  fileAlreadyExists: "ファイルは既に存在します",
  alreadyExistsIn: "{name} は既に {folder} にあります。",
  modified: "更新日時",
  existingFile: "既存のファイル",
  overwriteIfNewer: "新しい場合は上書き",
  keepBoth: "両方とも保持",
  localFile: "ローカルのファイル",
  serverFile: "サーバーのファイル",
  doTheSameForAny: "これらの転送でこの後に起きる競合にも同じ処理を行う",
  doTheSameForOthers: {
    other: "残りの {count} 件の競合とこの後に起きる競合にも同じ処理を行う",
  },

  // components/DeleteDialog
  nothingToDelete: "削除する項目はありません",
  deleteItem: "項目の削除",
  deleteItems: "{count} 個の項目の削除",
  checkingTheFolder: "フォルダーを確認しています...",
  theseItemsAreNoLonger: "これらの項目はもうフォルダーにありません。",

  // components/PropertiesDialog
  items: { other: "{count} 個の項目" },
  files: { other: "{count} 個のファイル" },
  folders: { other: "{count} 個のフォルダー" },
  countingFiles: {
    other: "集計中: {count} 個のファイル、{size}",
  },
  propertiesOf: "{name} のプロパティ",
  propertiesOfItems: "{count} 個の項目のプロパティ",
  items2: "項目",
  links: { other: "{count} 個のリンク" },
  type: "種類",
  location: "場所",
  pointsTo: "リンク先",
  calculate: "計算",
  accessed: "アクセス日時",
  mixedUnchanged: "混在 (変更しない)",
  permissions: "パーミッション",
  linksHaveNoPermissionsOf:
    "リンクには独自のパーミッションがないため、リンクのパーミッションは変わりません。所有者とグループの変更はリンク自体に適用されます。",
  andMoreListedInThe: "ほか {count} 件 (ログに記載)",
  changedItems: { other: "{count} 個の項目を変更しました" },
  octal: "8 進数",
  mixed: "混在",
  questionMarksAreLeftAs: "? の桁は、各項目の現在の値のまま残ります",
  owner: "所有者",
  group: "グループ",
  folders2: "フォルダー",
  files2: "ファイル",
  oneItemCouldNotBe: "1 個の項目を変更できませんでした:",
  itemsCouldNotBeChanged: "{count} 個の項目を変更できませんでした:",
  applying: "適用しています...",
  linkToAFolder: "フォルダーへのリンク",
  linkToAFile: "ファイルへのリンク",
  specialFile: "特殊ファイル",
  extensionFile: "{extension} ファイル",
  nameWithId: "{name} ({id})",
  measuringFolderSize: "フォルダーのサイズを計算中",
  changingPermissionsOf: "{what}のパーミッションを変更中",
  applyToEverythingInside: {
    other: "フォルダー内のすべての項目に適用",
  },
  permissionOfClass: "{who}の{permission}",

  // lib/deletion
  deletedCount: "{count} 個を削除済み",
  countingToDelete: "削除する項目を数えています...",
  deletedFoundSoFar: "{count} 個を削除済み (これまでに {found} 個を検出)",
  deletedOfTotal: "{total} 個中 {count} 個を削除済み",

  // lib/permissions
  setUserId: "SUID",
  runsAsTheFilesOwner: "ファイルの所有者の権限で実行します",
  setGroupId: "SGID",
  runsAsTheGroupNew: "グループの権限で実行します。新しいファイルはこのグループを継承します",
  sticky: "スティッキー",
  onlyOwnersMayDeleteOr: "所有者だけが中の項目を削除、名前変更できます",
  others: "その他",
  read: "読み取り",
  write: "書き込み",
  execute: "実行",
} satisfies TranslatedArea<"dialogs">;
