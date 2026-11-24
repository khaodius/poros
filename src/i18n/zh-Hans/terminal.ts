import type { TranslatedArea } from "../catalog";

export const terminal = {
  // components/TerminalPane
  couldNotReadTheClipboard: "无法读取剪贴板。请改用 Shift+Insert 粘贴。",
  selectAll: "全选",
  couldNotOpenATerminal: "无法打开终端",
  closeTab: "关闭标签页",
  restart: "重新启动",
  connecting: "正在连接...",
  connected: "已连接",
} satisfies TranslatedArea<"terminal">;
