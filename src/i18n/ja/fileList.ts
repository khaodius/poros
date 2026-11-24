import type { TranslatedArea } from "../catalog";

export const fileList = {
  // components/FileList
  expandGroup: "グループを展開",
  collapseGroup: "グループを折りたたむ",
  newNameFor: "{name} の新しい名前",
  resize: "{label}の幅を変更",
  dragToResizeDoubleClick: "ドラッグで幅を変更、ダブルクリックで内容に合わせます",
  nothingMatchesTheFilter: "フィルターに一致する項目はありません",
  onlyHiddenItemsHere: "非表示の項目のみです",
  thisFolderIsEmpty: "このフォルダーは空です",
  deletedTag: "削除済み",

  // components/PropertiesDialog
  folder: "フォルダー",
  brokenLink: "リンク切れ",
  file: "ファイル",

  // lib/columns
  type: "種類",
  modified: "更新日時",
  permissions: "パーミッション",
  owner: "所有者",

  // lib/dateGroups
  inTheFuture: "未来の日付",
  today: "今日",
  yesterday: "昨日",
  earlierThisWeek: "今週に入って (昨日より前)",
  lastWeek: "先週",
  earlierThisMonth: "今月に入って (先週より前)",
  lastMonth: "先月",
  earlierThisYear: "今年に入って (先月より前)",
  aLongTimeAgo: "かなり前",
  dateUnknown: "日付不明",
} satisfies TranslatedArea<"fileList">;
