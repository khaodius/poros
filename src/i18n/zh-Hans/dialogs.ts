import type { TranslatedArea } from "../catalog";

export const dialogs = {
  // components/ConflictDialog
  fileAlreadyExists: "文件已存在",
  alreadyExistsIn: "{folder} 中已存在 {name}。",
  modified: "修改日期",
  existingFile: "现有文件",
  overwriteIfNewer: "较新时覆盖",
  keepBoth: "保留两者",
  localFile: "本地文件",
  serverFile: "服务器文件",
  doTheSameForAny: "对这些传输中之后的所有冲突执行相同操作",
  doTheSameForOthers: {
    other: "对其他 {count} 个冲突及之后的所有冲突执行相同操作",
  },

  // components/DeleteDialog
  nothingToDelete: "没有可删除的内容",
  deleteItem: "删除项目",
  deleteItems: "删除 {count} 个项目",
  checkingTheFolder: "正在检查文件夹...",
  theseItemsAreNoLonger: "这些项目已不在该文件夹中。",

  // components/PropertiesDialog
  items: { other: "{count} 个项目" },
  files: { other: "{count} 个文件" },
  folders: { other: "{count} 个文件夹" },
  countingFiles: {
    other: "正在统计：{count} 个文件，{size}",
  },
  propertiesOf: "{name} 的属性",
  propertiesOfItems: "{count} 个项目的属性",
  items2: "项目",
  links: { other: "{count} 个链接" },
  type: "类型",
  location: "位置",
  pointsTo: "指向",
  calculate: "计算",
  accessed: "访问日期",
  mixedUnchanged: "不一致，保持不变",
  permissions: "权限",
  linksHaveNoPermissionsOf:
    "链接没有自己的权限，因此会保留原样。所有者和组的更改会应用于链接本身。",
  andMoreListedInThe: "以及另外 {count} 项，已列在日志中",
  changedItems: { other: "已更改 {count} 个项目" },
  octal: "八进制",
  mixed: "不一致",
  questionMarksAreLeftAs: "问号表示各项目保留其原有设置",
  owner: "所有者",
  group: "组",
  folders2: "文件夹",
  files2: "文件",
  oneItemCouldNotBe: "有 1 个项目无法更改：",
  itemsCouldNotBeChanged: "有 {count} 个项目无法更改：",
  applying: "正在应用...",
  linkToAFolder: "指向文件夹的链接",
  linkToAFile: "指向文件的链接",
  specialFile: "特殊文件",
  extensionFile: "{extension} 文件",
  nameWithId: "{name} ({id})",
  measuringFolderSize: "正在计算文件夹大小",
  changingPermissionsOf: "正在更改权限：{what}",
  applyToEverythingInside: {
    other: "应用于文件夹中的所有内容",
  },
  permissionOfClass: "{who}{permission}",

  // lib/deletion
  deletedCount: "已删除 {count} 项",
  countingToDelete: "正在统计要删除的内容...",
  deletedFoundSoFar: "已删除 {count} 项，目前已找到 {found} 项",
  deletedOfTotal: "已删除 {count} 项，共 {total} 项",

  // lib/permissions
  setUserId: "设置用户 ID",
  runsAsTheFilesOwner: "以文件所有者的身份运行",
  setGroupId: "设置组 ID",
  runsAsTheGroupNew: "以组的身份运行；新文件继承该组",
  sticky: "粘滞位",
  onlyOwnersMayDeleteOr: "只有所有者才能删除或重命名其中的项目",
  others: "其他人",
  read: "读取",
  write: "写入",
  execute: "执行",
} satisfies TranslatedArea<"dialogs">;
