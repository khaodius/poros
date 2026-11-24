import type { TranslatedArea } from "../catalog";

export const versions = {
  // components/VersionsDialog
  restoreThisVersion: "还原此版本",
  downloadThisVersion: "下载此版本",
  openInTheEditor: "在编辑器中打开",
  compareWithTheCurrentFile: "与当前文件比较",
  downloadingTheVersionFromTo: "正在将 {when} 的版本下载到 {folder}。可在传输队列中查看进度。",
  versionsOf: "{name} 的版本",
  loadingVersions: "正在加载版本...",
  turnOnVersioningForThe: "此存储桶不保留版本。为存储桶打开版本控制，即可保留被替换和删除的文件。",
  noVersionsOfThisFile: "未找到此文件的任何版本。",
  saved: "保存时间",
  status: "状态",
  storageClass: "存储类别",
  etag: "ETag",
  actions: "操作",
  version: "版本 {versionId}",
  entries: { other: "{count} 个条目" },
  restoreVersion: "还原版本",
  restore: "还原",
  makeTheVersionOf: "要将 {name} 在 {when} 的版本重新设为当前文件吗？",
  thisBringsTheDeletedFile: "这会恢复已删除的文件。删除记录仍保留在历史中。",
  theCurrentContentStaysIn: "当前内容会保留在历史中，因此之后也可以还原。",
  removeDeleteMarker: "移除删除标记",
  deleteVersion: "删除版本",
  removeMarker: "移除标记",
  removeTheDeleteMarkerOf: "要永久移除 {name} 在 {when} 的删除标记吗？此操作无法撤销。",
  deleteTheVersionOf: "要永久删除 {name} 在 {when} 的版本吗？此操作无法撤销。",
  deleteForGood: "永久删除",
  deletedItems: { other: "{count} 个已删除的项目" },
  everythingDeletedUnderAFolder:
    "文件夹下所有已删除的内容也会一并永久移除。仍在其中的文件不受影响。",
  andMore: { other: "以及另外 {count} 项" },
  removeThisDeleteMarker: "移除此删除标记",
  deleteThisVersionForGood: "永久删除此版本",
  versionFrom: "{when} 的版本",
  versionFromUnknownTime: "未知时间的版本",
  currentVersion: "当前版本",
  isNoLongerDeleted: "{name} 已不再处于删除状态。",
  theseItemsAreNoLongerDeleted: "这些项目已不再处于删除状态。",
  removeWithEveryVersion: "确定要永久移除 {what}，连同其保留的所有版本？此操作无法撤销。",

  // components/versionMenu
  versions: "版本...",
  versionsThisBucketKeepsNone: "版本（此存储桶不保留版本）",
  showDeletedFiles: "显示已删除的文件",
  showDeletedFilesThisBucket: "显示已删除的文件（此存储桶不保留版本）",
  restoreItems: { other: "还原 {count} 个项目" },
  copyPaths: "复制路径",
  copyPath: "复制路径",
  deleteItemsForGood: {
    other: "永久删除 {count} 个项目",
  },

  // hooks/usePaneVersions
  deletedFilesCannotBeShown: "无法显示已删除的文件：{message}",

  // lib/versions
  current: "当前",
  deleted: "已删除",
  deleteMarker: "删除标记",
  restoredCount: { other: "已还原 {count} 个" },
  findingWhatToRestore: "正在查找要还原的内容...",
  restoredFoundSoFar: {
    other: "已还原 {count} 个，目前已找到 {total} 个",
  },
  restoredOfTotal: { other: "已还原 {count} 个，共 {total} 个" },
  nothingOfTheFileIsLeft: "此后该文件将不复存在。",
  theFileComesBackAsItsNewestVersion: "文件将以其最新版本恢复。",
  theFileStaysDeleted: "文件仍处于删除状态：此标记下还有另一个删除标记。",
  thisIsTheOnlyVersion: "这是唯一的版本：文件将不复存在。",
  theVersionBeforeItBecomesCurrent: "它之前的版本将成为当前文件。",
  theFileWillShowAsDeleted: "文件将显示为已删除：它之前的条目是删除标记。",
  unknownTime: "未知时间",
  versioningIsSuspended:
    "此存储桶的版本控制已暂停：已保留的内容仍会保留，新的写入会替换标记为 null 的版本。",
  theseKeysMayNotSeeVersioning:
    "这些密钥可能无法查看存储桶是否保留版本，因此某些操作可能会被拒绝。",

  // state/versionActions
  downloadAVersionOfTo: "将 {name} 的版本下载到",
  restoringItemsIn: {
    other: "正在还原 {folder} 中的 {count} 个项目",
  },
  restoringNameIn: "正在还原 {folder} 中的 {name}",
  deletingItemsForGoodIn: {
    other: "正在永久删除 {folder} 中的 {count} 个项目",
  },
  deletingNameForGoodIn: "正在永久删除 {folder} 中的 {name}",
  filesCouldNotBeRestored: {
    other: "{count} 个文件无法还原。原因见日志。",
  },
  filesCouldNotBeDeleted: {
    other: "{count} 个文件无法删除。原因见日志。",
  },
} satisfies TranslatedArea<"versions">;
