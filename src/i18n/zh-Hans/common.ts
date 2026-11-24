import type { TranslatedArea } from "../catalog";

export const common = {
  // App
  settings: "设置",

  // components/AppearanceSettings
  delete: "删除",
  name: "名称",

  // components/ArchiveSettings
  browse: "浏览",
  listSeparator: "，",

  // components/AutomationSettings
  refresh: "刷新",
  add: "添加",
  labelWithMessage: "{label}：{message}",

  // components/BulkRenameDialog
  sentenceSeparator: "",
  start: "开始",
  tryAgain: "重试",
  cancel: "取消",
  rename: "重命名",
  remove: "移除",
  keep: "保留",
  no: "否",

  // components/CommandOutputDialog
  stop: "停止",
  close: "关闭",

  // components/ConfirmDialog
  working: "正在处理...",

  // components/ConflictDialog
  size: "大小",
  skip: "跳过",
  overwrite: "覆盖",
  resume: "继续",

  // components/ConnectDialog
  connect: "连接",
  save: "保存",

  // components/DriveStatus
  discard: "放弃",
  open: "打开",

  // components/EditorPane
  dismiss: "关闭",

  // components/ExternalEditDialogs
  upload: "上传",

  // components/ExternalEditorSettings
  minutesUnit: "分钟",

  // components/FileList
  folder: "文件夹",

  // components/FileOperationsHost
  replace: "替换",

  // components/FilePane
  asSentence: "{text}。",
  none: "无",
  cut: "剪切",
  copy: "复制",
  paste: "粘贴",
  edit: "编辑",
  loading: "正在加载...",

  // components/FontPicker
  default: "默认",

  // components/ImportConnectionsDialog
  clauseSeparator: "；",
  back: "返回",

  // components/LimitScheduleSettings
  download: "下载",
  pause: "暂停",

  // components/PreviewPane
  nothing: "无",

  // components/PropertiesDialog
  apply: "应用",
  ok: "确定",

  // components/SearchBar
  search: "搜索",

  // components/SettingsDialog
  askMe: "询问我",
  secondsUnit: "秒",

  // components/SyncPlanList
  path: "路径",

  // components/TerminalPane
  clear: "清除",

  // components/TransferView
  retry: "重试",

  // components/UpdateNotice
  hide: "隐藏",

  // lib/format
  durationSeconds: "{seconds} 秒",
  durationMinutes: "{minutes} 分 {seconds} 秒",
  durationHours: "{hours} 小时 {minutes} 分",
} satisfies TranslatedArea<"common">;
