import type { TranslatedArea } from "../catalog";

export const transfers = {
  // components/TransferLookSettings
  timeLeft: "剩余 {duration}",

  // components/TransferView
  copySourcePaths: "复制源路径",
  copySourcePath: "复制源路径",
  moveToTop: "移到顶部",
  moveToBottom: "移到底部",
  progress: "进度",
  speed: "速度",
  status: "状态",
  dragFilesBetweenTabsOr: "在标签页之间拖动文件，或使用文件菜单中的“上传”和“下载”。",
  transfersThatFailAfterTheir: "重试后仍失败的传输会显示在这里。",
  finishedTransfersAreListedHere: "已完成的传输会列在这里。",
  done: "完成",
  scheduleThese: "计划这些传输...",
  scheduleThis: "计划此传输...",
  connectionLostWaitingForTheServer: "连接已断开，正在等待服务器",
  waitingToRetry: "等待重试（第 {attempt} 次尝试）",
  queued: "排队中",
  listingFolder: "正在列出文件夹",
  updatingWithRsyncSent: "正在使用 rsync 更新，已发送 {size}",
  copyingDirectlyBetweenTheServers: "正在服务器之间直接复制",
  transferringOnConnections: {
    other: "正在通过 {count} 个连接传输",
  },
  transferring: "正在传输",
  paused: "已暂停",
  alreadyExistsWaitingForYourChoice: "已存在，等待您的选择",
  doneCopiedDirectlyBetweenTheServers: "完成，已在服务器之间直接复制",
  doneWithRsyncSent: "已使用 rsync 完成，已发送 {sent}，共 {size}",
  skippedBecause: "已跳过：{error}",
  skipped: "已跳过",
  failedBecause: "失败：{error}",
  failedUnknownError: "失败：未知错误",
  rowTitle: "{source}\n到 {target}",
  rowTitleQueuedByTask: "{source}\n到 {target}\n由任务 {task} 加入队列",

  // lib/progressBars
  sizeOfSize: "{transferred} / {size}",

  // state/transferActions
  nothingToTransferOnlyRegular: "没有可传输的内容：只能复制普通文件和文件夹。",
  unlockTheVaultToCopy: "解锁保险库后才能将文件复制到其中。",
  couldNotCheckWhetherThis: "无法检查此文件夹是否为保险库。",
  isALockedVaultOpen: "{baseName} 是已锁定的保险库。打开它后才能将文件复制到其中。",
  dropLocalFilesOnA: "将本地文件拖放到服务器标签页上即可上传。",
  copyingTo: "正在复制到 {folder}",
  movedTo: "已移动到 {folder}",
  copiedTo: "已复制到 {folder}",
  connectToAServerTo: "连接到服务器后才能上传。",
  openALocalTabTo: "打开一个本地标签页作为下载位置。",
  dropFilesOnAServer: "将文件拖放到服务器标签页上即可上传。",
  local: "本地",
} satisfies TranslatedArea<"transfers">;
