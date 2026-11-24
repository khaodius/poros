import type { TranslatedArea } from "../catalog";

export const workspace = {
  // App
  connectToServer: "连接到服务器",
  synchronizeFolders: "同步文件夹",
  scheduledTasks: "计划任务",
  settingsCtrl: "设置 (Ctrl+,)",

  // components/BottomPanel
  queue: "队列",
  failed: "失败",
  completed: "已完成",
  startQueue: "开始队列",
  pauseQueue: "暂停队列",
  pauseSelected: "暂停所选项",
  resumeSelected: "继续所选项",
  moveSelectedToTheTop: "将所选项移到顶部",
  moveSelectedToTheBottom: "将所选项移到底部",
  removeSelected: "移除所选项",
  retryAll: "全部重试",
  retrySelected: "重试所选项",
  clearFailedTransfers: "清除失败的传输",
  keepEachTransferInView: "每项传输完成时滚动到该项",
  follow: "跟随",
  clearFinishedTransfers: "清除已完成的传输",
  simultaneousTransfers: "同时传输数",
  workers: "并发数",
  log: "日志",

  // components/CloseTabDialog
  close: "关闭 {label}？",
  transfersIntoOrOutOf:
    "无论选择哪项，进出已解锁保险库的传输都会暂停：关闭标签页会锁定保险库。再次解锁保险库后可继续这些传输。",
  pauseTransfersAndClose: "暂停传输并关闭",
  keepTransferring: "继续传输",
  transfersForThisServerStillQueued: {
    other:
      "此服务器仍有 {count} 项传输在队列中。关闭标签页后，传输可以使用单独的连接继续运行，也可以暂停等待，直到您继续。",
  },
  unmountAndClose: "卸载并关闭",

  // components/DockArea
  dragToResizeDoubleClick: "拖动以调整大小，双击以平均分配",
  moveToMainWindow: "移到主窗口",
  newWindow: "新窗口",
  moveToThatWindow: "移到该窗口",

  // components/DragGhost
  uploadTo: "上传到 {folder}",
  downloadTo: "下载到 {folder}",
  copyTo: "复制到 {folder}",
  copyTo2: "复制到 {baseName}",
  moveTo: "移动到 {baseName}",

  // components/DropMarker
  splitRight: "向右拆分",
  splitDown: "向下拆分",
  mergeAsTab: "合并为标签页",
  splitLeft: "向左拆分",
  splitUp: "向上拆分",

  // components/RestoringPane
  signIn: "登录",
  closeTab: "关闭标签页",
  reopensNameOnceConnected: "连接后重新打开 {name}。",
  opensANewShellOnceConnected: "连接后打开新的 shell。",
  reopensOnceConnected: "连接后重新打开。",
  openingName: "正在打开 {name}...",
  openingFile: "正在打开文件...",
  reconnectingTo: "正在重新连接到 {label}...",
  signInTo: "登录到 {label}",
  couldNotReconnectTo: "无法重新连接到 {label}",
  porosKeepsNoPasswordForThisServer: "Poros 未保存此服务器的密码。",

  // components/SplashScreen
  loadingPoros: "正在加载 Poros",

  // components/StatusBar
  connectionToLost: "与 {label} 的连接已断开",
  connectedTo: "已连接到 {label}",
  serversConnected: { other: "已连接 {count} 台服务器" },
  notConnected: "未连接",
  updateTo: "更新到 {version}",
  uploadSpeed: "上传速度",
  downloadSpeed: "下载速度",
  appVersion: "Poros {version}",
  transfersLeft: { other: "剩余 {count} 项传输" },
  aboutDuration: "约 {duration}",
  queueStatusPaused: "{status}（已暂停）",

  // components/TabBar
  openTerminal: "打开终端",
  moveToNewWindow: "移到新窗口",
  closeOtherTabs: "关闭其他标签页",
  newTabCtrlT: "新建标签页 (Ctrl+T)",
  newTab: "新建标签页",
  disconnected: "已断开连接",
  unsavedChanges: "有未保存的更改",
  close2: "关闭 {label}",
  closeCtrlW: "关闭 (Ctrl+W)",
  mountAsDrive: "挂载为驱动器...",
  mountedAsADriveAt: "已挂载为驱动器：{paths}",
  mountedAsADrive: "已挂载为驱动器",

  // components/TabContent
  local: "本地",
  thisSessionHasEnded: "此会话已结束",
  connectionLost: "连接已断开",
  reconnectingInS: "{secondsLeft} 秒后重新连接",
  reconnecting: "正在重新连接...",
  reconnectingInSecondsAttempt: "{seconds} 秒后重新连接（第 {attempt} 次尝试）",
  reconnectingAttempt: "正在重新连接...（第 {attempt} 次尝试）",
  reconnectNow: "立即重新连接",
  reconnect: "重新连接",

  // components/WelcomePanel
  localFiles: "本地文件",
  import: "导入",
  importFromFilezillaWinscpAnd: "从 FileZilla、WinSCP 和其他应用导入",
  savedConnections: "已保存的连接",
  serversYouSaveFromThe:
    "在连接对话框中或通过服务器标签页的书签按钮保存的服务器会显示在这里。可以导入在 FileZilla、WinSCP、PuTTY 和其他应用中保存的站点。",
  connecting: "正在连接...",

  // components/WindowControls
  minimize: "最小化",
  restore: "还原",
  maximize: "最大化",

  // components/Workspace
  dragToResizeDoubleClickTo: "拖动以调整大小，双击以重置",

  // hooks/useTabLabel
  closedSession: "已关闭的会话",

  // state/restoreActions
  couldNotReopen: "无法重新打开 {name}：{message}",
  porosNoLongerKnowsHow: "Poros 已不知道如何访问它。",
} satisfies TranslatedArea<"workspace">;
