import type { TranslatedArea } from "../catalog";

export const drives = {
  // components/DriveStatus
  filesNotUploaded: { other: "{count} 个文件未上传" },
  drivesMountedFromPoros: "从 Poros 挂载的驱动器",
  mountedDrives: "已挂载的驱动器",
  driveAtMountPoint: "{label}，位于 {mountPoint}",
  readOnly: "只读",
  factAt: "位置",
  factShows: "显示",
  factActivity: "活动",
  uploads: "上传",
  unmount: "卸载",
  refusedWhileProgramsHaveFiles: "程序在驱动器上有打开的文件或有待上传的内容时，无法卸载",
  forceUnmount: "强制卸载",
  unmountsAtOnceProgramsLose:
    "立即卸载：程序会失去已打开的文件，尚未上传的内容会保留在这台计算机上",
  notUploaded: "未上传",
  notUploadedKeptOnThis: "未上传，已保留在这台计算机上",
  keptFileFacts: "{connection}，{size}",
  keptFileFactsWithError: "{connection}，{size}：{error}",
  saveACopy: "保存副本...",
  discardForGood: "永久放弃",

  // components/DrivesGoing
  unmountedWhenTheTabCloses: {
    other: "关闭标签页时，将卸载位于 {paths} 的驱动器。",
  },
  unmountedWhenTheVaultLocks: {
    other: "锁定保险库时，将卸载位于 {paths} 的驱动器。",
  },
  unmountedWhenPorosQuits: {
    other: "退出 Poros 时，将卸载位于 {paths} 的驱动器。",
  },
  busyDrivesLoseOpenFiles:
    "仍有程序在其中打开着文件，或有待上传的内容。这些程序会失去已打开的文件，尚未上传到服务器的内容会保留在这台计算机上，并列在状态栏的驱动器下。",
  busyDrivesLoseOpenFilesWithVault:
    "仍有程序在其中打开着文件，或有待上传的内容。这些程序会失去已打开的文件，尚未上传到服务器的内容会保留在这台计算机上，并列在状态栏的驱动器下。从保险库保留的文件在您保存或放弃之前以未加密形式存储。",

  // components/MountDialog
  mountNameAsADrive: "将 {name} 挂载为驱动器",
  theServersRoot: "服务器的根目录",
  theVaultsRoot: "保险库的根目录",
  rootAt: "{root}：{path}",
  whatProgramsSaveServer:
    "程序保存的内容会在其关闭文件时上传。关闭标签页或退出 Poros 时，驱动器会被卸载。",
  whatProgramsSaveVault:
    "程序保存的内容会在其关闭文件时上传，在此之前以未加密形式保存在这台计算机上。关闭标签页、锁定保险库或退出 Poros 时，驱动器会被卸载。",
  mountAsDrive: "挂载为驱动器",
  theConnectionHasClosed: "连接已关闭。",
  driveLetter: "驱动器号",
  folderOnThisComputer: "这台计算机上的文件夹",
  aFreeLetterTheDrive: "用于显示驱动器的空闲驱动器号。",
  anEmptyFolderMadeIf: "一个空文件夹，不存在时会创建，之后再删除。",
  showOnTheDrive: "在驱动器中显示",
  thisFolder: "此文件夹",
  readOnlyProgramsCanOpen: "只读：程序可以打开文件，但不能更改",
  openInFileManagerWhen: "挂载后在文件管理器中打开",
  theConnectionIsLostReconnect: "连接已断开。重新连接后才能挂载。",
  mounting: "正在挂载...",
  mount: "挂载",

  // components/QuitDialog
  quitPoros: "退出 Poros？",
  unmountAndQuit: "卸载并退出",

  // lib/mounts
  restartUnmountsDrivesInUse: {
    other:
      "{count} 个驱动器上有程序打开的文件或待上传的内容。重启会卸载这些驱动器，尚未上传的内容会保留在这台计算机上。",
  },
  restartUnmountsDrivesInUseWithVault: {
    other:
      "{count} 个驱动器上有程序打开的文件或待上传的内容。重启会卸载这些驱动器，尚未上传的内容会保留在这台计算机上，来自保险库的文件以未加密形式保存。",
  },
  driveCount: { other: "{count} 个驱动器" },
  drivesWithFailedUploads: {
    other: "{drives}，{count} 个上传失败",
  },
  drivesUploading: { other: "{drives}，正在上传 {count} 个" },
  filesOpen: { other: "{count} 个文件已打开" },
  uploadsPending: { other: "{count} 个待上传" },
  sizeRead: "已读取 {size}",
  sizeWritten: "已写入 {size}",
  idle: "空闲",
  uploadFailed: "失败：{error}",
  uploadWaiting: "等待中",
  uploading: "正在上传",
  uploadingPercent: "正在上传，{percent}%",
  chooseADriveLetter: "选择一个驱动器号。",
  chooseAFolder: "选择一个文件夹。",
  driveLetterSuchAs: "驱动器号，例如 {example}",
  fullPathSuchAs: "完整路径，例如 {example}",

  // state/mountActions
  savedACopyAt: "已将副本保存到 {destination}",
} satisfies TranslatedArea<"drives">;
