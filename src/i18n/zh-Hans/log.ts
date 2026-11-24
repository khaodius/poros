import type { TranslatedArea } from "../catalog";

export const log = {
  // components/LogView
  saveLog: "保存日志",
  levels: "级别",
  session: "会话",
  allSessions: "所有会话",
  searchLog: "搜索日志",
  clearSearch: "清除搜索",
  showTimes: "显示时间",
  wrapLongLines: "长行自动换行",
  followNewLines: "跟随新行",
  copyShownLines: "复制显示的行",
  saveShownLinesToA: "将显示的行保存到文件",
  clearLog: "清除日志",
  info: "信息",
  warnings: "警告",
  errors: "错误",
  server: "服务器",
  closedSession: "已关闭的会话 {id}",
  connectionAndTransferActivityAppears: "连接和传输活动会显示在这里。",
  noLinesMatchTheFilters: "没有与筛选条件匹配的行。",
  logFiles: "日志文件",
} satisfies TranslatedArea<"log">;
