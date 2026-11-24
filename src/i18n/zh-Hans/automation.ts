import type { TranslatedArea } from "../catalog";

export const automation = {
  // components/AutomationSettings
  whenTheTransferQueueFinishes: "传输队列完成时",
  then: "然后",
  thenHint: "也可在队列的工具栏中设置。除运行命令外，所有操作都会先倒计时 30 秒，以便您取消。",
  onlyTheNextTime: "仅限下一次",
  onlyTheNextTimeHint: "执行一次后恢复为“不执行任何操作”。",
  command: "命令",
  commandHint:
    "用于“运行命令”；在这台计算机上运行。{done}、{failed}、{skipped} 和 {bytes} 包含各项总数。",
  aCommandOrScript: "命令或脚本",
  showANotification: "显示通知",
  onlyWhenPorosIsIn: "仅当 Poros 在后台时",
  playASound: "播放声音",
  serverCommands: "服务器命令",
  listedUnderCommandsWhenYou:
    "在服务器窗格中右键单击时，列在“命令”下。它们通过服务器的 shell 在当前显示的文件夹中运行，并填入以下内容：",
  scheduledTasks: "计划任务",
  synchronizeFoldersOrRunCommands: "在设定的时间同步文件夹或运行命令",
  synchronizeFoldersOrRunCommandsHint: "任务在 Poros 打开时于后台运行，并使用已保存的连接。",
  output: "输出",
  openAWindowWithWhat: "打开一个窗口显示命令的输出；关闭时静默运行",
  reloadTheServersFoldersWhen: "命令完成后重新加载服务器的文件夹",
  deleteCommand: "删除命令",
  nameOfTheNewCommand: "新命令的名称",
  newCommand: "新建命令",

  // components/CommandOutputDialog
  earlierOutputWasDropped: "较早的输出已被丢弃。",
  theCommandPrintedNothing: "该命令没有任何输出。",
  running: "正在运行",
  commandIn: "在 {folder} 中运行 {command}",
  commandsIn: { other: "在 {folder} 中运行 {count} 个命令" },
  theLoginFolder: "登录文件夹",

  // components/RunCommandDialog
  runACommand: "运行命令",
  runACommandOnThe: "在服务器上运行命令",
  commandToRunIn: "要在 {folder} 中运行的命令",
  refreshTheServersFoldersWhen: "完成后刷新服务器的文件夹",
  selectFilesOrFoldersTo: "选择文件或文件夹以填入占位符。",
  run: "运行",

  // components/WhenDoneControl
  whatHappensWhenTheQueue: "队列完成时执行的操作",
  whenDone: "完成后",
  whenTheQueueFinishes: "队列完成时",

  // components/WhenDoneCountdown
  queueFinishedClosingPorosIn: "传输队列已完成。{seconds}后关闭 Poros。",
  queueFinishedLockingIn: "传输队列已完成。{seconds}后锁定计算机。",
  queueFinishedSleepingIn: "传输队列已完成。{seconds}后使计算机进入睡眠状态。",
  queueFinishedHibernatingIn: "传输队列已完成。{seconds}后使计算机休眠。",
  queueFinishedLoggingOffIn: "传输队列已完成。{seconds}后注销。",
  queueFinishedShuttingDownIn: "传输队列已完成。{seconds}后关闭计算机。",
  seconds: { other: "{count} 秒" },

  // components/commandMenu
  commands: "命令",
  runACommand2: "运行命令...",
  editCommands: "编辑命令...",

  // lib/automation
  filesTransferredIn: {
    other: "已传输 {count} 个文件（{size}），用时 {duration}",
  },
  doNothing: "不执行任何操作",
  closePoros: "关闭 Poros",
  lockTheComputer: "锁定计算机",
  sleep: "睡眠",
  hibernate: "休眠",
  logOff: "注销",
  shutDown: "关机",
  closingPoros: "关闭 Poros",
  lockingTheComputer: "锁定计算机",
  puttingTheComputerToSleep: "使计算机进入睡眠状态",
  hibernatingTheComputer: "使计算机休眠",
  loggingOff: "注销",
  shuttingDownTheComputer: "关闭计算机",
  confirmCloseNow: "立即关闭",
  confirmLockNow: "立即锁定",
  confirmSleepNow: "立即睡眠",
  confirmHibernateNow: "立即休眠",
  confirmLogOffNow: "立即注销",
  confirmShutDownNow: "立即关机",
  transfersFinishedWithErrors: "传输已完成，但有错误",
  transfersFinished: "传输已完成",
  failedCount: { other: "{count} 个失败" },
  skippedCount: { other: "{count} 个已跳过" },

  // lib/commands
  eachSelectedItemsFullPath: "每个所选项目的完整路径，每个项目运行一次",
  eachSelectedItemsNameRunning: "每个所选项目的名称，每个项目运行一次",
  theFullPathsOfAll: "所有所选项目的完整路径",
  theNamesOfAllSelected: "所有所选项目的名称",
  theFolderShown: "当前显示的文件夹",
  stoppedAfter: "运行 {elapsed} 后已停止",
  endedBySignal: "运行 {elapsed} 后被信号 {signal} 终止",
  endedWithoutStatus: "运行 {elapsed} 后结束，没有退出状态",
  exitedWithStatus: "运行 {elapsed} 后退出，状态为 {status}",
  finishedIn: "已完成，用时 {elapsed}",

  // state/commandStore
  stopped: "已停止",
  nothingToRun: "没有要运行的内容",
  allCommandsFinished: { other: "全部 {count} 个命令已完成" },
  ofCommandsFailed: { other: "{finished} 个命令中有 {count} 个失败" },

  // state/whenDone
  whenTheQueueFinished: "队列完成时",
  theCommandAfterTheQueue: "队列完成后运行的命令",
} satisfies TranslatedArea<"automation">;
