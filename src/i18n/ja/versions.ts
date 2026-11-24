import type { TranslatedArea } from "../catalog";

export const versions = {
  // components/VersionsDialog
  restoreThisVersion: "このバージョンを復元",
  downloadThisVersion: "このバージョンをダウンロード",
  openInTheEditor: "エディターで開く",
  compareWithTheCurrentFile: "現在のファイルと比較",
  downloadingTheVersionFromTo:
    "{when} のバージョンを {folder} にダウンロードしています。進行状況は転送キューに表示されます。",
  versionsOf: "{name} のバージョン",
  loadingVersions: "バージョンを読み込んでいます...",
  turnOnVersioningForThe:
    "このバケットはバージョンを保持していません。置き換えや削除されたファイルを残すには、バケットのバージョニングをオンにしてください。",
  noVersionsOfThisFile: "このファイルのバージョンは見つかりませんでした。",
  saved: "保存日時",
  status: "状態",
  storageClass: "ストレージクラス",
  etag: "ETag",
  actions: "操作",
  version: "バージョン {versionId}",
  entries: { other: "{count} 件" },
  restoreVersion: "バージョンの復元",
  restore: "復元",
  makeTheVersionOf: "{when} の {name} のバージョンを、もう一度現在のファイルにしますか?",
  thisBringsTheDeletedFile: "削除されたファイルが戻ります。削除の記録は履歴に残ります。",
  theCurrentContentStaysIn: "現在の内容は履歴に残るため、後で復元することもできます。",
  removeDeleteMarker: "削除マーカーの削除",
  deleteVersion: "バージョンの削除",
  removeMarker: "マーカーを削除",
  removeTheDeleteMarkerOf:
    "{when} の {name} の削除マーカーを完全に削除しますか? この操作は元に戻せません。",
  deleteTheVersionOf:
    "{when} の {name} のバージョンを完全に削除しますか? この操作は元に戻せません。",
  deleteForGood: "完全に削除",
  deletedItems: { other: "削除された {count} 個の項目" },
  everythingDeletedUnderAFolder:
    "フォルダーの下で削除されたものもすべて対象になります。フォルダー内に残っているファイルは変更されません。",
  andMore: { other: "ほか {count} 個" },
  removeThisDeleteMarker: "この削除マーカーを削除",
  deleteThisVersionForGood: "このバージョンを完全に削除",
  versionFrom: "{when} のバージョン",
  versionFromUnknownTime: "日時不明のバージョン",
  currentVersion: "現在のバージョン",
  isNoLongerDeleted: "{name} は削除済みではなくなりました。",
  theseItemsAreNoLongerDeleted: "これらの項目は削除済みではなくなりました。",
  removeWithEveryVersion:
    "{what} と、保持されているすべてのバージョンを完全に削除しますか? この操作は元に戻せません。",

  // components/versionMenu
  versions: "バージョン...",
  versionsThisBucketKeepsNone: "バージョン (このバケットは保持していません)",
  showDeletedFiles: "削除されたファイルを表示",
  showDeletedFilesThisBucket:
    "削除されたファイルを表示 (このバケットはバージョンを保持していません)",
  restoreItems: { other: "{count} 個の項目を復元" },
  copyPaths: "パスをコピー",
  copyPath: "パスをコピー",
  deleteItemsForGood: {
    other: "{count} 個の項目を完全に削除",
  },

  // hooks/usePaneVersions
  deletedFilesCannotBeShown: "削除されたファイルを表示できません: {message}",

  // lib/versions
  current: "現在",
  deleted: "削除済み",
  deleteMarker: "削除マーカー",
  restoredCount: { other: "{count} 個を復元済み" },
  findingWhatToRestore: "復元する項目を探しています...",
  restoredFoundSoFar: {
    other: "{count} 個を復元済み (これまでに {total} 個を検出)",
  },
  restoredOfTotal: { other: "{total} 個中 {count} 個を復元済み" },
  nothingOfTheFileIsLeft: "この後、ファイルは何も残りません。",
  theFileComesBackAsItsNewestVersion: "ファイルは最新のバージョンとして戻ります。",
  theFileStaysDeleted: "ファイルは削除されたままです。この下に別の削除マーカーがあります。",
  thisIsTheOnlyVersion: "これが唯一のバージョンです。ファイルはなくなります。",
  theVersionBeforeItBecomesCurrent: "1 つ前のバージョンが現在のファイルになります。",
  theFileWillShowAsDeleted:
    "ファイルは削除済みとして表示されます。1 つ前の項目が削除マーカーです。",
  unknownTime: "日時不明",
  versioningIsSuspended:
    "このバケットではバージョニングが停止されています。保持済みのものは残り、新しい書き込みは null のバージョンを置き換えます。",
  theseKeysMayNotSeeVersioning:
    "このキーではバケットがバージョンを保持しているかを確認できない可能性があるため、一部の操作が拒否されることがあります。",

  // state/versionActions
  downloadAVersionOfTo: "{name} のバージョンのダウンロード先",
  restoringItemsIn: {
    other: "{folder} の {count} 個の項目を復元中",
  },
  restoringNameIn: "{folder} の {name} を復元中",
  deletingItemsForGoodIn: {
    other: "{folder} の {count} 個の項目を完全に削除中",
  },
  deletingNameForGoodIn: "{folder} の {name} を完全に削除中",
  filesCouldNotBeRestored: {
    other: "{count} 個のファイルを復元できませんでした。理由はログに記録されています。",
  },
  filesCouldNotBeDeleted: {
    other: "{count} 個のファイルを削除できませんでした。理由はログに記録されています。",
  },
} satisfies TranslatedArea<"versions">;
