import type { TranslatedArea } from "../catalog";

export const fileList = {
  // components/FileList
  expandGroup: "展开组",
  collapseGroup: "折叠组",
  newNameFor: "{name} 的新名称",
  resize: "调整“{label}”的宽度",
  dragToResizeDoubleClick: "拖动以调整大小，双击以自动调整",
  nothingMatchesTheFilter: "没有与筛选条件匹配的项目",
  onlyHiddenItemsHere: "此处只有隐藏的项目",
  thisFolderIsEmpty: "此文件夹为空",
  deletedTag: "已删除",

  // components/PropertiesDialog
  folder: "文件夹",
  brokenLink: "损坏的链接",
  file: "文件",

  // lib/columns
  type: "类型",
  modified: "修改日期",
  permissions: "权限",
  owner: "所有者",

  // lib/dateGroups
  inTheFuture: "将来",
  today: "今天",
  yesterday: "昨天",
  earlierThisWeek: "本周早些时候",
  lastWeek: "上周",
  earlierThisMonth: "本月早些时候",
  lastMonth: "上个月",
  earlierThisYear: "今年早些时候",
  aLongTimeAgo: "很久以前",
  dateUnknown: "日期未知",
} satisfies TranslatedArea<"fileList">;
