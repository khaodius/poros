import type { TranslatedArea } from "../catalog";

export const common = {
  // App
  settings: "設定",

  // components/AppearanceSettings
  delete: "削除",
  name: "名前",

  // components/ArchiveSettings
  browse: "参照",

  // components/AutomationSettings
  refresh: "更新",
  add: "追加",

  // components/BulkRenameDialog
  start: "開始",
  tryAgain: "再試行",
  cancel: "キャンセル",
  rename: "名前の変更",
  remove: "削除",
  keep: "そのまま",
  no: "なし",

  // components/CommandOutputDialog
  stop: "停止",
  close: "閉じる",

  // components/ConfirmDialog
  working: "処理中...",

  // components/ConflictDialog
  size: "サイズ",
  skip: "スキップ",
  overwrite: "上書き",
  resume: "再開",

  // components/ConnectDialog
  connect: "接続",
  save: "保存",

  // components/DriveStatus
  discard: "破棄",
  open: "開く",

  // components/DrivesGoing
  listSeparator: "、",

  // components/EditorPane
  dismiss: "閉じる",

  // components/ExternalEditDialogs
  upload: "アップロード",

  // components/FileList
  folder: "フォルダー",

  // components/FileOperationsHost
  replace: "置き換え",

  // components/FilePane
  none: "なし",
  cut: "切り取り",
  copy: "コピー",
  paste: "貼り付け",
  edit: "編集",
  loading: "読み込み中...",

  // components/FontPicker
  default: "既定",

  // components/ImportConnectionsDialog
  back: "戻る",

  // components/LimitScheduleSettings
  download: "ダウンロード",
  pause: "一時停止",

  // components/PreviewPane
  nothing: "なし",

  // components/PropertiesDialog
  apply: "適用",

  // components/SearchBar
  search: "検索",

  // components/SettingsDialog
  askMe: "毎回確認",

  // components/SyncPlanList
  path: "パス",

  // components/TerminalPane
  clear: "クリア",

  // components/TransferView
  retry: "再試行",

  // components/UpdateNotice
  hide: "隠す",

  // lib/format
  durationSeconds: "{seconds} 秒",
  durationMinutes: "{minutes} 分 {seconds} 秒",
  durationHours: "{hours} 時間 {minutes} 分",
} satisfies TranslatedArea<"common">;
